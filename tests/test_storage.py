from studypilot.storage import Store


def test_account_crud_and_task_cache(tmp_path):
    store = Store(tmp_path / "test.db")
    account = store.add_account("pigai", "tester", "secret", "测试账号")
    assert account["username"] == "tester"
    assert store.get_account(account["id"], include_secret=True)["password"] == "secret"

    store.replace_account_data(
        account["id"],
        [
            {
                "platform": "pigai",
                "id": "pigai:all",
                "name": "批改网作业",
                "progress": 90,
                "raw": {},
            }
        ],
        [
            {
                "platform": "pigai",
                "course_id": "pigai:all",
                "course_name": "批改网作业",
                "title": "第1号 作文",
                "task_id": "1",
                "status": "pending",
                "deadline": "2026-11-11 23:59",
                "url": "https://www.pigai.org/",
                "detail": "作文",
                "source": "wait-complete",
                "raw": {},
            }
        ],
    )
    tasks = store.list_tasks()
    assert len(tasks) == 1
    assert tasks[0]["title"] == "第1号 作文"

    store.replace_account_data(account["id"], [], [])
    assert store.list_tasks() == []
    assert store.delete_account(account["id"]) is True


def test_reminder_lifecycle(tmp_path):
    store = Store(tmp_path / "test.db")
    reminder = store.add_reminder("完成作文", "2026-11-11T20:00")
    assert reminder["enabled"] == 1
    assert store.update_reminder(reminder["id"], enabled=False)
    assert store.get_reminder(reminder["id"])["enabled"] == 0
    assert store.delete_reminder(reminder["id"])


def test_upsert_task_merges_lookup(tmp_path):
    store = Store(tmp_path / "test.db")
    account = store.add_account("pigai", "tester", "secret")
    task_id = store.upsert_task(
        account["id"],
        {
            "platform": "pigai",
            "course_id": "pigai:all",
            "course_name": "批改网作业",
            "title": "第3457245号 作文",
            "task_id": "3457245",
            "status": "pending",
            "deadline": "2027-11-11 23:59",
            "url": "https://www.pigai.org/",
            "detail": "作文号 3457245",
            "source": "essay-lookup",
            "raw": {},
        },
    )
    assert task_id
    assert len(store.list_tasks()) == 1
    store.upsert_task(
        account["id"],
        {
            "platform": "pigai",
            "course_id": "pigai:all",
            "course_name": "批改网作业",
            "title": "第3457245号 作文",
            "task_id": "3457245",
            "status": "completed",
            "deadline": "2027-11-11 23:59",
            "url": "https://www.pigai.org/",
            "detail": "已完成",
            "source": "essay-lookup",
            "raw": {},
        },
    )
    tasks = store.list_tasks()
    assert len(tasks) == 1
    assert tasks[0]["status"] == "completed"


def test_completed_history_and_restore(tmp_path):
    store = Store(tmp_path / "test.db")
    account = store.add_account("pigai", "tester", "secret")
    task_id = store.upsert_task(
        account["id"],
        {
            "platform": "pigai",
            "course_id": "pigai:all",
            "course_name": "批改网作业",
            "title": "第1号 作文",
            "task_id": "1",
            "status": "pending",
            "deadline": "2027-11-11 23:59",
            "url": "https://www.pigai.org/",
            "detail": "作文号 1",
            "source": "essay-lookup",
            "raw": {},
        },
    )
    assert store.update_task_status(task_id, "completed")
    completed = store.list_completed_tasks()
    assert len(completed) == 1
    assert completed[0]["title"] == "第1号 作文"
    assert completed[0]["status"] == "completed"

    assert store.update_task_status(task_id, "pending")
    assert store.list_completed_tasks() == []
    assert store.list_tasks()[0]["status"] == "pending"


def test_completed_history_purges_older_than_seven_days(tmp_path):
    store = Store(tmp_path / "test.db")
    account = store.add_account("pigai", "tester", "secret")
    task_id = store.upsert_task(
        account["id"],
        {
            "platform": "pigai",
            "course_id": "pigai:all",
            "course_name": "批改网作业",
            "title": "旧任务",
            "task_id": "2",
            "status": "pending",
            "deadline": None,
            "url": "",
            "detail": "",
            "source": "",
            "raw": {},
        },
    )
    store.update_task_status(task_id, "completed")
    with store._lock, store._connect() as conn:
        conn.execute(
            "UPDATE completed_tasks SET completed_at = ?",
            (store._now() - 8 * 24 * 60 * 60,),
        )
        conn.commit()
    assert store.list_completed_tasks(days=7) == []

