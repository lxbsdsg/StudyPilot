"""Application service coordinating adapters, persistence, and scheduling."""

from __future__ import annotations

import logging
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timedelta
from typing import Callable

from .adapters import create_adapter
from .adapters.base import PlatformAuthError, PlatformError
from .config import PLATFORM_LABELS
from .reminders import ReminderService
from .reminder_time import normalize_reminder_mode
from .storage import Store

logger = logging.getLogger(__name__)


class StudyPilotService:
    def __init__(self, store: Store | None = None) -> None:
        self.store = store or Store()
        self.events: list[dict] = []
        self._event_lock = threading.Lock()
        self.reminders = ReminderService(self.store, on_event=self._record_event)
        self.executor = ThreadPoolExecutor(
            max_workers=8,
            thread_name_prefix="studypilot",
        )
        self._refresh_lock = threading.Lock()
        self._seed_platform_checks()

    def _seed_platform_checks(self) -> None:
        checks = self.store.get_platform_checks()
        defaults = {
            "pigai": (
                "verified",
                "已用测试账号验证登录与待完成列表读取",
            ),
            "unipus": (
                "endpoint_checked",
                "登录端点已在线验证，等待真实账号完成课程抓取",
            ),
            "chaoxing": (
                "endpoint_checked",
                "登录端点已在线验证，等待真实账号完成课程与作业抓取",
            ),
            "welearn": (
                "needs_account",
                "SSO 已改版为浏览器流程，等待真实账号验证",
            ),
        }
        for platform, (status, note) in defaults.items():
            if platform not in checks:
                self.store.set_platform_check(platform, status, note)

    def start(self) -> None:
        self.reminders.start()

    def stop(self) -> None:
        self.reminders.stop()
        self.executor.shutdown(wait=False, cancel_futures=False)

    def _record_event(self, event: dict) -> None:
        event = {**event, "ts": int(datetime.now().timestamp() * 1000)}
        with self._event_lock:
            self.events.append(event)
            self.events = self.events[-200:]

    def list_events(self, after_ms: int = 0) -> list[dict]:
        with self._event_lock:
            return [event for event in self.events if event.get("ts", 0) > after_ms]

    def dashboard(self) -> dict:
        accounts = self.store.list_accounts()
        tasks = self.store.list_tasks()
        reminders = self.store.list_reminders()
        pending = [task for task in tasks if task["status"] != "completed"]
        due_soon = []
        now = datetime.now()
        horizon = now + timedelta(days=3)
        for task in pending:
            if not task.get("deadline"):
                continue
            try:
                deadline = datetime.fromisoformat(str(task["deadline"]))
            except ValueError:
                continue
            if now <= deadline <= horizon:
                due_soon.append(task)
        by_platform: dict[str, dict] = {}
        for account in accounts:
            platform = account["platform"]
            item = by_platform.setdefault(
                platform,
                {
                    "platform": platform,
                    "label": PLATFORM_LABELS.get(platform, platform),
                    "accounts": 0,
                    "pending": 0,
                    "errors": 0,
                },
            )
            item["accounts"] += 1
            item["errors"] += int(account["status"] == "error")
        for task in pending:
            item = by_platform.setdefault(
                task["platform"],
                {
                    "platform": task["platform"],
                    "label": PLATFORM_LABELS.get(task["platform"], task["platform"]),
                    "accounts": 0,
                    "pending": 0,
                    "errors": 0,
                },
            )
            item["pending"] += 1
        return {
            "accounts": len(accounts),
            "courses": len(self.store.list_courses()),
            "pending": len(pending),
            "due_soon": len(due_soon),
            "reminders": len([item for item in reminders if item["enabled"]]),
            "platforms": list(by_platform.values()),
            "platform_checks": self.store.get_platform_checks(),
            "next_tasks": pending[:8],
            "upcoming_reminders": reminders[:6],
        }

    def add_account(
        self,
        platform: str,
        username: str,
        password: str,
        label: str = "",
        refresh: bool = True,
    ) -> dict:
        if platform not in PLATFORM_LABELS:
            raise ValueError(f"不支持的平台：{platform}")
        if not username.strip():
            raise ValueError("账号不能为空")
        if not password:
            raise ValueError("密码不能为空")
        account = self.store.add_account(
            platform,
            username.strip(),
            password,
            label.strip(),
        )
        if refresh:
            self.refresh_account(account["id"])
        return self.store.get_account(account["id"]) or account

    def refresh_account(self, account_id: str) -> dict:
        account = self.store.get_account(account_id, include_secret=True)
        if not account:
            raise ValueError("账号不存在")
        self.store.update_account(account_id, status="refreshing", last_error="")
        adapter = None
        try:
            adapter = create_adapter(account["platform"], account)
            if not (account.get("session") or {}).get("cookies"):
                adapter.login()
            result = adapter.refresh()
            session = adapter.export_session()
            if result.account:
                self.store.update_account(
                    account_id,
                    display_name=result.account.display_name,
                    user_id=result.account.user_id,
                    school=result.account.school,
                    session=session,
                    status="ready",
                    last_error="",
                    last_refresh=int(datetime.now().timestamp()),
                )
            else:
                self.store.update_account(
                    account_id,
                    session=session,
                    status="ready",
                    last_error="",
                    last_refresh=int(datetime.now().timestamp()),
                )
            self.store.replace_account_data(
                account_id,
                [course.to_dict() for course in result.courses],
                [task.to_dict() for task in result.tasks],
            )
            self.store.set_platform_check(
                account["platform"],
                "verified",
                "该平台已有账号成功完成登录、课程与任务读取",
            )
            self._record_event(
                {
                    "type": "account.refreshed",
                    "account_id": account_id,
                    "platform": account["platform"],
                    "tasks": len(result.tasks),
                }
            )
            return {
                "ok": True,
                "tasks": len(result.tasks),
                "courses": len(result.courses),
                "warning": result.warning,
            }
        except PlatformAuthError as exc:
            self.store.update_account(
                account_id,
                status="auth_error",
                last_error=str(exc),
            )
            raise
        except Exception as exc:
            logger.exception("Account refresh failed: %s", account_id)
            self.store.update_account(
                account_id,
                status="error",
                last_error=str(exc),
            )
            if isinstance(exc, (PlatformError, ValueError)):
                raise
            raise PlatformError(str(exc)) from exc
        finally:
            if adapter is not None:
                adapter.close()

    def lookup_pigai_essay(
        self,
        account_id: str,
        essay_id: str,
    ) -> dict:
        account = self.store.get_account(account_id, include_secret=True)
        if not account:
            raise ValueError("账号不存在")
        if account["platform"] != "pigai":
            raise ValueError("只有批改网账号支持按作文号查询")
        adapter = None
        try:
            adapter = create_adapter(account["platform"], account)
            if not (account.get("session") or {}).get("cookies"):
                adapter.login()
                self.store.update_account(
                    account_id,
                    session=adapter.export_session(),
                    status="ready",
                    last_error="",
                )
            task = adapter.lookup_essay(essay_id)
            self.store.upsert_task(account_id, task.to_dict())
            self._record_event(
                {
                    "type": "pigai.essay_lookup",
                    "account_id": account_id,
                    "essay_id": essay_id,
                    "title": task.title,
                }
            )
            return task.to_dict()
        except Exception as exc:
            logger.exception("Pigai essay lookup failed")
            if isinstance(exc, (PlatformError, ValueError)):
                raise
            raise PlatformError(str(exc)) from exc
        finally:
            if adapter is not None:
                adapter.close()

    def refresh_all(self, account_ids: list[str] | None = None) -> list[dict]:
        accounts = self.store.list_accounts()
        if account_ids:
            allowed = set(account_ids)
            accounts = [item for item in accounts if item["id"] in allowed]
        results: list[dict] = []
        with ThreadPoolExecutor(max_workers=min(8, max(1, len(accounts)))) as pool:
            futures = {
                pool.submit(self.refresh_account, account["id"]): account
                for account in accounts
            }
            for future in as_completed(futures):
                account = futures[future]
                try:
                    result = future.result()
                    results.append(
                        {
                            "account_id": account["id"],
                            "platform": account["platform"],
                            "ok": True,
                            **result,
                        }
                    )
                except Exception as exc:
                    results.append(
                        {
                            "account_id": account["id"],
                            "platform": account["platform"],
                            "ok": False,
                            "error": str(exc),
                        }
                    )
        return results

    def create_task_reminder(
        self,
        task_id: str,
        remind_at: str | None = None,
        offset_minutes: int = 60,
        mode: str = "countdown",
    ) -> dict:
        tasks = self.store.list_tasks()
        task = next((item for item in tasks if item["id"] == task_id), None)
        if not task:
            raise ValueError("任务不存在")
        when = normalize_reminder_mode(
            mode,
            remind_at=remind_at,
            deadline=task.get("deadline"),
            minutes_before=offset_minutes,
        ).strftime("%Y-%m-%dT%H:%M")
        reminder = self.store.add_reminder(
            title=f"{PLATFORM_LABELS.get(task['platform'], task['platform'])}：{task['title']}",
            remind_at=when,
            account_id=task["account_id"],
            task_id=task_id,
        )
        self.reminders.reload()
        return reminder

