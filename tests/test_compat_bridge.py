from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

from studypilot import compat_bridge
from studypilot.adapters.base import PlatformError
from studypilot.models import AccountInfo, Course, RefreshResult, StudyTask


class FakeAdapter:
    def __init__(self, account):
        self.account = account
        self.closed = False
        self.login_called = False

    def login(self):
        self.login_called = True
        return {"ok": True}

    def refresh(self):
        return RefreshResult(
            courses=[
                Course(
                    id="course-1",
                    name="课程一",
                    progress=42,
                    platform=self.account["platform"],
                    raw={"id": "course-1"},
                )
            ],
            tasks=[
                StudyTask(
                    platform=self.account["platform"],
                    course_id="course-1",
                    course_name="课程一",
                    title="任务一",
                    task_id="task-1",
                    deadline="2026-12-01 23:59",
                    url="https://example.com/task",
                    detail="未完成",
                    source="unit",
                )
            ],
            account=AccountInfo(
                platform=self.account["platform"],
                username=self.account["username"],
                display_name="测试用户",
                school="测试大学",
            ),
        )

    def export_session(self):
        return {"version": 1, "cookies": [{"name": "auth", "value": "new"}]}

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        self.closed = True


def request() -> dict:
    return {
        "protocol_version": 1,
        "platform": "welearn",
        "username": "student",
        "password": "secret",
        "session": {"version": 1, "cookies": []},
    }


def test_bridge_returns_structured_snapshot_without_persistence(monkeypatch):
    adapter = None

    def create(platform, account):
        nonlocal adapter
        adapter = FakeAdapter(account)
        return adapter

    monkeypatch.setattr(compat_bridge, "create_adapter", create)
    result = compat_bridge.refresh(request())

    assert adapter is not None and adapter.closed
    assert adapter.login_called
    assert result["session"]["cookies"][0]["value"] == "new"
    assert result["account"]["display_name"] == "测试用户"
    assert result["courses"][0]["progress"] == 42
    assert result["tasks"][0]["task_id"] == "task-1"
    assert result["tasks"][0]["deadline"] == "2026-12-01 23:59"
    assert "password" not in json.dumps(result)


@pytest.mark.parametrize("platform", ["pigai", "unknown", ""])
def test_bridge_rejects_platforms_outside_allowlist(platform):
    payload = request()
    payload["platform"] = platform
    with pytest.raises(ValueError, match="平台不允许"):
        compat_bridge.refresh(payload)


@pytest.mark.parametrize(
    "payload",
    [
        [],
        {"protocol_version": 2, "platform": "welearn"},
        {"protocol_version": 1, "platform": "welearn", "username": "", "password": ""},
        {"protocol_version": 1, "platform": "welearn", "username": "a", "password": None},
    ],
)
def test_bridge_rejects_invalid_requests(payload):
    with pytest.raises(ValueError):
        compat_bridge.refresh(payload)


def test_bridge_redacts_adapter_failures(monkeypatch):
    def fail_refresh(self):
        raise PlatformError("登录态已失效")

    monkeypatch.setattr(FakeAdapter, "refresh", fail_refresh)
    monkeypatch.setattr(compat_bridge, "create_adapter", lambda platform, account: FakeAdapter(account))
    with pytest.raises(PlatformError, match="登录态已失效"):
        compat_bridge.refresh(request())


def test_cli_accepts_framework_prefixed_utf8_and_redacts_credentials(tmp_path):
    root = Path(__file__).resolve().parents[1]
    request_path = tmp_path / "request.json"
    request_path.write_bytes(
        b"\xef\xbb\xbf"
        + json.dumps(
            {**request(), "platform": "unknown", "password": "泄漏标记"}
        ).encode()
    )
    process = subprocess.Popen(
        [sys.executable, "-m", "studypilot.compat_bridge"],
        cwd=root,
        stdin=request_path.open("rb"),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env={**os.environ, "COVERAGE_PROCESS_START": ""},
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    stdout, stderr = process.communicate(timeout=10)
    assert process.returncode in {0, 1}, stderr
    output = stdout
    assert "平台不允许" in output or "Python 兼容适配器失败" in output
    assert "泄漏标记" not in output
    assert "password" not in output
