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

