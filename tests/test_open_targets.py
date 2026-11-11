from studypilot.open_targets import (
    desktop_url,
    is_mobile_user_agent,
    mobile_targets,
)
from studypilot.service import StudyPilotService


def test_mobile_user_agent_detection():
    assert is_mobile_user_agent("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0)")
    assert is_mobile_user_agent("Mozilla/5.0 (Linux; Android 14)")
    assert not is_mobile_user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")


def test_desktop_url_always_uses_platform_homepage():
    assert desktop_url("pigai", "https://www.pigai.org/essay/1") == (
        "https://www.pigai.org/"
    )
    assert desktop_url("pigai") == "https://www.pigai.org/"


def test_mobile_targets_use_official_website_only():
    target = mobile_targets("pigai", "https://www.pigai.org/essay/1")
    assert target == {
        "kind": "mobile",
        "platform": "pigai",
        "web_url": "https://www.pigai.org/zt/mobile/",
        "desktop_url": "https://www.pigai.org/",
    }


def test_other_platform_mobile_targets_use_homepage():
    target = mobile_targets("chaoxing", "https://mooc1.chaoxing.com/task")
    assert target["web_url"] == "https://mooc1.chaoxing.com/"
    assert target["desktop_url"] == "https://mooc1.chaoxing.com/"


class FakeStore:
    account = {
        "id": "account-1",
        "platform": "chaoxing",
        "username": "student",
    }

    def get_account(self, account_id, include_secret=False):
        return self.account if account_id == self.account["id"] else None

    def list_tasks(self, account_id=None):
        assert account_id == self.account["id"]
        return [
            {
                "id": "task-1",
                "account_id": account_id,
                "platform": "chaoxing",
                "title": "作业",
                "url": "https://mooc1.chaoxing.com/task",
            }
        ]


def make_service():
    service = StudyPilotService.__new__(StudyPilotService)
    service.store = FakeStore()
    return service


def test_task_open_target_does_not_return_account_identity():
    target = make_service().task_open_target("account-1", "task-1")
    assert target["url"] == "https://mooc1.chaoxing.com/"
    assert "account_id" not in target


def test_mobile_task_open_target_uses_official_mobile_homepage():
    target = make_service().task_open_target(
        "account-1",
        "task-1",
        user_agent="Mozilla/5.0 (iPhone; CPU iPhone OS 17_0)",
    )
    assert target["web_url"] == "https://mooc1.chaoxing.com/"
    assert "account_id" not in target
