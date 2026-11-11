from fastapi.testclient import TestClient

from studypilot.app import create_app


class FakeStore:
    account = {"id": "account-1", "platform": "pigai"}

    def get_account(self, account_id):
        return self.account if account_id == self.account["id"] else None

    def list_tasks(self, account_id=None):
        return [
            {
                "id": "task-1",
                "account_id": self.account["id"],
                "platform": "pigai",
                "title": "作文",
                "url": "https://www.pigai.org/essay/1",
            }
        ]


class FakeService:
    def __init__(self):
        self.store = FakeStore()

    def start(self):
        pass

    def stop(self):
        pass

    def task_open_target(self, account_id, task_id, mobile=False):
        assert account_id == self.store.account["id"]
        assert task_id == "task-1"
        return {"kind": "website", "url": "https://www.pigai.org/"}


def test_legacy_open_link_redirects_to_official_homepage_without_cookies():
    with TestClient(create_app(FakeService())) as client:
        response = client.get("/open/account-1/task-1", follow_redirects=False)

    assert response.status_code == 307
    assert response.headers["location"] == "https://www.pigai.org/"
    assert "set-cookie" not in response.headers


def test_open_api_returns_official_website_target():
    with TestClient(create_app(FakeService())) as client:
        response = client.post(
            "/api/open",
            json={"account_id": "account-1", "task_id": "task-1", "mobile": False},
        )

    assert response.status_code == 200
    assert response.json()["url"] == "https://www.pigai.org/"
