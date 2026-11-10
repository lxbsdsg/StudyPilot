"""FastAPI application for the StudyPilot desktop workbench."""

from __future__ import annotations

from pathlib import Path

from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import HTMLResponse, RedirectResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel

from .config import PLATFORM_LABELS, VERSION
from .open_targets import desktop_url
from .service import StudyPilotService

WEB_DIR = Path(__file__).parent / "web"


class AccountCreate(BaseModel):
    platform: str
    username: str
    password: str
    label: str = ""


class ReminderCreate(BaseModel):
    title: str
    remind_at: str
    account_id: str | None = None
    task_id: str | None = None
    repeat_rule: str = "once"


class ReminderUpdate(BaseModel):
    title: str | None = None
    remind_at: str | None = None
    repeat_rule: str | None = None
    enabled: bool | None = None


class TaskReminderCreate(BaseModel):
    mode: str = "countdown"
    remind_at: str | None = None
    offset_minutes: int = 60


class PigaiEssayLookup(BaseModel):
    account_id: str
    essay_id: str


class OpenRequest(BaseModel):
    account_id: str
    task_id: str
    mobile: bool = False


def create_app(service: StudyPilotService | None = None) -> FastAPI:
    app = FastAPI(title="StudyPilot", version=VERSION)
    service = service or StudyPilotService()
    app.state.service = service
    service.start()

    @app.on_event("shutdown")
    def shutdown() -> None:
        service.stop()

    @app.get("/", include_in_schema=False)
    def index() -> HTMLResponse:
        return HTMLResponse(
            (WEB_DIR / "index.html").read_text(encoding="utf-8"),
            headers={"Cache-Control": "no-cache"},
        )

    app.mount("/static", StaticFiles(directory=WEB_DIR), name="static")

    @app.get("/api/health")
    def health() -> dict:
        return {"ok": True, "version": VERSION}

    @app.get("/api/platforms")
    def platforms() -> list[dict]:
        checks = service.store.get_platform_checks()
        return [
            {
                "id": platform,
                "label": label,
                "check": checks.get(platform, {}),
            }
            for platform, label in PLATFORM_LABELS.items()
        ]

    @app.get("/api/dashboard")
    def dashboard() -> dict:
        return service.dashboard()

    @app.get("/api/events")
    def events(after: int = 0) -> list[dict]:
        return service.list_events(after)

    @app.get("/api/accounts")
    def accounts() -> list[dict]:
        return service.store.list_accounts()

    @app.post("/api/accounts", status_code=201)
    def create_account(payload: AccountCreate) -> dict:
        try:
            return service.add_account(
                payload.platform,
                payload.username,
                payload.password,
                payload.label,
            )
        except Exception as exc:
            raise HTTPException(400, str(exc)) from exc

    @app.delete("/api/accounts/{account_id}", status_code=204)
    def delete_account(account_id: str) -> None:
        if not service.store.delete_account(account_id):
            raise HTTPException(404, "账号不存在")

    @app.post("/api/accounts/{account_id}/refresh")
    def refresh_account(account_id: str) -> dict:
        try:
            return service.refresh_account(account_id)
        except Exception as exc:
            raise HTTPException(400, str(exc)) from exc

    @app.post("/api/accounts/refresh")
    def refresh_all(payload: dict | None = None) -> list[dict]:
        account_ids = payload.get("account_ids") if payload else None
        return service.refresh_all(account_ids)

    @app.get("/api/courses")
    def courses(account_id: str | None = None) -> list[dict]:
        return service.store.list_courses(account_id)

    @app.get("/api/tasks")
    def tasks(
        account_id: str | None = None,
        status: str | None = None,
        platform: str | None = None,
    ) -> list[dict]:
        return service.store.list_tasks(account_id, status, platform)

    @app.get("/api/completed-tasks")
    def completed_tasks(days: int = 7) -> list[dict]:
        return service.store.list_completed_tasks(days)

    @app.post("/api/pigai/essay")
    def pigai_essay(payload: PigaiEssayLookup) -> dict:
        try:
            return service.lookup_pigai_essay(
                payload.account_id,
                payload.essay_id,
            )
        except Exception as exc:
            raise HTTPException(400, str(exc)) from exc

    @app.post("/api/open")
    def open_target(payload: OpenRequest, request: Request) -> dict:
        try:
            return service.task_open_target(
                payload.account_id,
                payload.task_id,
                request.headers.get("user-agent", ""),
                payload.mobile,
            )
        except Exception as exc:
            raise HTTPException(400, str(exc)) from exc

    @app.get("/open/{account_id}/{task_id}")
    def open_proxy(
        account_id: str,
        task_id: str,
        request: Request,
    ):
        account = service.store.get_account(account_id)
        if not account:
            raise HTTPException(404, "账号不存在")
        tasks = service.store.list_tasks(account_id=account_id)
        task = next((item for item in tasks if item["id"] == task_id), None)
        if not task:
            raise HTTPException(404, "任务不存在")
        url = task.get("url") or desktop_url(account["platform"])
        headers = service.account_open_headers(account_id)
        if not headers:
            return RedirectResponse(url, status_code=307)
        response = service.http_get_with_headers(url, headers)
        return HTMLResponse(
            response.text,
            status_code=response.status_code,
            headers={"Cache-Control": "no-store"},
        )

    @app.post("/api/tasks/{task_id}/status")
    def task_status(task_id: str, payload: dict) -> dict:
        status = str(payload.get("status") or "")
        if status not in {"pending", "completed", "ignored"}:
            raise HTTPException(400, "无效状态")
        if not service.store.update_task_status(task_id, status):
            raise HTTPException(404, "任务不存在")
        return {"ok": True}

    @app.post("/api/completed-tasks/{task_id}/restore")
    def restore_completed_task(task_id: str) -> dict:
        if not service.store.update_task_status(task_id, "pending"):
            raise HTTPException(404, "已完成任务不存在")
        return {"ok": True}

    @app.post("/api/tasks/{task_id}/reminder", status_code=201)
    def task_reminder(task_id: str, payload: TaskReminderCreate) -> dict:
        try:
            return service.create_task_reminder(
                task_id,
                payload.remind_at,
                payload.offset_minutes,
                payload.mode,
            )
        except Exception as exc:
            raise HTTPException(400, str(exc)) from exc

    @app.get("/api/reminders")
    def reminders() -> list[dict]:
        return service.store.list_reminders()

    @app.post("/api/reminders", status_code=201)
    def create_reminder(payload: ReminderCreate) -> dict:
        reminder = service.store.add_reminder(
            payload.title,
            payload.remind_at,
            payload.account_id,
            payload.task_id,
            payload.repeat_rule,
        )
        service.reminders.reload()
        return reminder

    @app.patch("/api/reminders/{reminder_id}")
    def update_reminder(reminder_id: str, payload: ReminderUpdate) -> dict:
        fields = payload.model_dump(exclude_none=True)
        if not service.store.update_reminder(reminder_id, **fields):
            raise HTTPException(404, "提醒不存在")
        service.reminders.reload()
        return service.store.get_reminder(reminder_id) or {}

    @app.delete("/api/reminders/{reminder_id}", status_code=204)
    def delete_reminder(reminder_id: str) -> None:
        if not service.store.delete_reminder(reminder_id):
            raise HTTPException(404, "提醒不存在")
        service.reminders.reload()

    return app

