"""Private JSON bridge for the Rust core's Python compatibility adapters.

The bridge only fetches remote data. Persistence remains under the Rust core so
that two processes never write the shared SQLite database during a refresh.
"""

from __future__ import annotations

import json
import sys
from dataclasses import asdict
from typing import Any

from .adapters import PlatformError, create_adapter
from .models import AccountInfo, Course, RefreshResult, StudyTask

ALLOWED_PLATFORMS = frozenset({"unipus", "welearn", "chaoxing"})
PROTOCOL_VERSION = 1


def _text(value: Any, fallback: str = "") -> str:
    return value if isinstance(value, str) else fallback


def _progress(value: Any) -> int:
    try:
        return int(value or 0)
    except (TypeError, ValueError):
        return 0


def _safe_raw(value: Any) -> dict:
    return value if isinstance(value, dict) else {}


def _session_value(adapter: Any) -> dict:
    try:
        value = adapter.export_session()
    except Exception:
        return {}
    return value if isinstance(value, dict) else {}


def _result_value(
    platform: str,
    username: str,
    result: RefreshResult,
    session: dict,
    sensitive_values: tuple[str, ...] = (),
) -> dict:
    courses = []
    for course in result.courses:
        if not isinstance(course, Course):
            continue
        courses.append(
            {
                "platform": platform,
                "id": str(course.id),
                "name": str(course.name or "未命名课程"),
                "progress": _progress(course.progress),
                "raw": _safe_raw(course.raw),
            }
        )

    tasks = []
    for task in result.tasks:
        if not isinstance(task, StudyTask):
            continue
        tasks.append(
            {
                "platform": platform,
                "course_id": str(task.course_id),
                "course_name": str(task.course_name),
                "title": str(task.title or "未命名任务"),
                "task_id": str(task.task_id),
                "status": (
                    task.status
                    if task.status in {"pending", "completed", "ignored"}
                    else "pending"
                ),
                "deadline": task.deadline or None,
                "url": str(task.url),
                "detail": str(task.detail),
                "source": str(task.source or "task-list"),
                "raw": _safe_raw(task.raw),
            }
        )

    account = result.account
    if isinstance(account, AccountInfo):
        account_value = {
            "platform": platform,
            "username": username,
            "display_name": str(account.display_name or username),
            "user_id": str(account.user_id),
            "school": str(account.school),
            "extra": _safe_raw(account.extra),
        }
    else:
        account_value = {
            "platform": platform,
            "username": username,
            "display_name": username,
            "user_id": "",
            "school": "",
            "extra": {},
        }
    response = {
        "courses": courses,
        "tasks": tasks,
        "account": account_value,
        "warning": str(result.warning),
        "session": session,
    }
    return _redact_values(response, sensitive_values)


def _redact_text(value: str, sensitive_values: tuple[str, ...]) -> str:
    for secret in sensitive_values:
        if secret:
            value = value.replace(secret, "[REDACTED]")
    return value


def _redact_values(value: Any, sensitive_values: tuple[str, ...]) -> Any:
    if not sensitive_values:
        return value
    if isinstance(value, str):
        return _redact_text(value, sensitive_values)
    if isinstance(value, dict):
        return {
            key: _redact_values(item, sensitive_values) for key, item in value.items()
        }
    if isinstance(value, list):
        return [_redact_values(item, sensitive_values) for item in value]
    return value


def refresh(payload: Any) -> dict:
    if not isinstance(payload, dict):
        raise ValueError("请求必须是 JSON 对象")
    if payload.get("protocol_version") != PROTOCOL_VERSION:
        raise ValueError("不支持的桥接协议版本")
    platform = payload.get("platform")
    if platform not in ALLOWED_PLATFORMS:
        raise ValueError("平台不允许通过兼容桥接刷新")
    username = payload.get("username")
    password = payload.get("password")
    if not isinstance(username, str) or not username:
        raise ValueError("登录账号不能为空")
    if not isinstance(password, str):
        raise ValueError("登录密码必须是字符串")
    session = payload.get("session")
    account = {
        "platform": platform,
        "username": username,
        "password": password,
        "session": session if isinstance(session, dict) else {},
    }
    with create_adapter(platform, account) as adapter:
        try:
            adapter.login()
        except Exception:
            # Existing sessions can remain valid when a headless login attempt
            # is blocked by an upstream challenge; refresh will still validate it.
            if not account["session"].get("cookies"):
                raise
        result = adapter.refresh()
        session_value = _session_value(adapter)
    sensitive_values = (password,) if password else ()
    return _result_value(platform, username, result, session_value, sensitive_values)


def main() -> int:
    response: dict = {"ok": False, "protocol_version": PROTOCOL_VERSION}
    try:
        request = json.loads(sys.stdin.buffer.read().decode("utf-8-sig"))
        response.update({"ok": True, "result": refresh(request)})
    except Exception as error:
        sensitive_values = (
            (request.get("password"),)
            if isinstance(request, dict) and isinstance(request.get("password"), str)
            else ()
        )
        detail = (
            _redact_text(str(error), sensitive_values).strip()
            or error.__class__.__name__
        )
        if isinstance(error, PlatformError):
            response["error"] = detail
        else:
            response["error"] = f"Python 兼容适配器失败：{detail}"
    sys.stdout.write(
        json.dumps(response, ensure_ascii=False, separators=(",", ":"))
    )
    sys.stdout.flush()
    return 0 if response["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
