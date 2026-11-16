import threading

from studypilot.models import Course, RefreshResult, StudyTask
from studypilot.service import PreparedRefresh, StudyPilotService
from studypilot.storage import Store


def test_refresh_result_is_committed_once(tmp_path):
    store = Store(tmp_path / "test.db")
    account = store.add_account("pigai", "tester", "secret")
    prepared = PreparedRefresh(
        status="ready",
        last_error="",
        session={"version": 1, "cookies": [{"name": "auth", "value": "ok"}]},
        display_name="Tester",
        user_id="42",
        school="School",
        last_refresh=123,
        result=RefreshResult(
            courses=[Course("pigai:all", "Pigai", 90, "pigai")],
            tasks=[
                StudyTask(
                    platform="pigai",
                    course_id="pigai:all",
                    course_name="Pigai",
                    title="Essay",
                    task_id="1",
                )
            ],
        ),
    )
    service = StudyPilotService.__new__(StudyPilotService)
    service.store = store
    service._refresh_lock = threading.RLock()
    service._event_lock = threading.Lock()
    service.events = []
    service.apply_refresh(account["id"], "pigai", prepared)

    updated = store.get_account(account["id"])
    assert updated["display_name"] == "Tester"
    assert updated["session"]["cookies"][0]["name"] == "auth"
    assert updated["last_refresh"] == 123
    assert store.list_tasks()[0]["title"] == "Essay"
    assert service.events[-1]["type"] == "account.refreshed"


def test_failed_refresh_keeps_last_successful_tasks(tmp_path):
    store = Store(tmp_path / "test.db")
    account = store.add_account("pigai", "tester", "secret")
    old_session = {
        "version": 1,
        "cookies": [{"name": "_JUKU_USER", "value": "old"}],
    }
    store.update_account(account["id"], session=old_session)
    store.replace_account_data(
        account["id"],
        [{"platform": "pigai", "id": "pigai:all", "name": "Pigai", "progress": 50}],
        [
            {
                "platform": "pigai",
                "course_id": "pigai:all",
                "course_name": "Pigai",
                "title": "Cached task",
                "task_id": "1",
            }
        ],
    )
    prepared = PreparedRefresh(
        status="auth_error",
        last_error="Session expired",
        session={"version": 1, "cookies": []},
        display_name=None,
        user_id=None,
        school=None,
        last_refresh=0,
        result=ValueError("Session expired"),
    )
    service = StudyPilotService.__new__(StudyPilotService)
    service.store = store
    service._refresh_lock = threading.RLock()
    service._event_lock = threading.Lock()
    service.events = []
    service.apply_refresh(account["id"], "pigai", prepared)

    updated = store.get_account(account["id"])
    assert updated["status"] == "auth_error"
    assert updated["last_error"] == "Session expired"
    assert updated["session"] == old_session
    assert store.list_tasks()[0]["title"] == "Cached task"
    assert not service.events
