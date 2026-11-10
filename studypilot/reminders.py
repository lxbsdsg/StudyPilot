"""Background reminder scheduler with desktop notifications."""

from __future__ import annotations

import logging
import subprocess
import sys
import threading
from datetime import datetime
from typing import Callable

from apscheduler.schedulers.background import BackgroundScheduler

from .storage import Store

logger = logging.getLogger(__name__)


def notify(title: str, message: str) -> None:
    if sys.platform == "win32":
        try:
            from win10toast import ToastNotifier

            ToastNotifier().show_toast(title, message, duration=10, threaded=True)
            return
        except Exception:
            pass
        try:
            script = (
                "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, "
                "ContentType=WindowsRuntime] > $null;"
                "$template = [Windows.UI.Notifications.ToastNotificationManager]"
                "::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]"
                "::ToastText02);"
                "$texts = $template.GetElementsByTagName('text');"
                f"$texts.Item(0).AppendChild($template.CreateTextNode('{title}')) > $null;"
                f"$texts.Item(1).AppendChild($template.CreateTextNode('{message}')) > $null;"
                "$toast = [Windows.UI.Notifications.ToastNotification]::new($template);"
                "[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("
                "'StudyPilot').Show($toast);"
            )
            subprocess.Popen(
                [
                    "powershell",
                    "-NoProfile",
                    "-WindowStyle",
                    "Hidden",
                    "-Command",
                    script,
                ],
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            return
        except Exception:
            logger.exception("Windows notification failed")

    try:
        subprocess.Popen(
            ["notify-send", title, message],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except Exception:
        logger.info("REMINDER %s: %s", title, message)


class ReminderService:
    def __init__(
        self,
        store: Store,
        on_event: Callable[[dict], None] | None = None,
    ) -> None:
        self.store = store
        self.on_event = on_event
        self.scheduler = BackgroundScheduler(timezone="Asia/Shanghai")
        self._lock = threading.Lock()

    def start(self) -> None:
        if not self.scheduler.running:
            self.scheduler.start()
        self.reload()

    def stop(self) -> None:
        if self.scheduler.running:
            self.scheduler.shutdown(wait=False)

    def reload(self) -> None:
        with self._lock:
            for job in self.scheduler.get_jobs():
                if job.id.startswith("reminder:"):
                    job.remove()
            now = datetime.now()
            for reminder in self.store.list_reminders():
                if not reminder["enabled"]:
                    continue
                try:
                    run_at = datetime.fromisoformat(
                        str(reminder["remind_at"]).replace("Z", "+00:00")
                    ).replace(tzinfo=None)
                except ValueError:
                    continue
                if run_at <= now and reminder.get("repeat_rule") == "once":
                    continue
                trigger = "date" if reminder.get("repeat_rule") == "once" else "interval"
                kwargs: dict = {}
                if reminder.get("repeat_rule") == "daily":
                    kwargs = {"hours": 24}
                elif reminder.get("repeat_rule") == "weekly":
                    kwargs = {"weeks": 1}
                elif reminder.get("repeat_rule") == "weekdays":
                    kwargs = {"days": 1}
                else:
                    kwargs = {"run_date": run_at}
                self.scheduler.add_job(
                    self._fire,
                    trigger=trigger,
                    id=f"reminder:{reminder['id']}",
                    replace_existing=True,
                    kwargs={"reminder_id": reminder["id"]},
                    **kwargs,
                )

    def _fire(self, reminder_id: str) -> None:
        reminder = self.store.get_reminder(reminder_id)
        if not reminder or not reminder["enabled"]:
            return
        title = "StudyPilot 学习提醒"
        message = reminder["title"]
        notify(title, message)
        repeat = reminder.get("repeat_rule") or "once"
        weekday_only = repeat == "weekdays"
        disable = repeat == "once" or (
            weekday_only and datetime.now().weekday() >= 5
        )
        self.store.mark_reminder_triggered(reminder_id, disable=disable)
        if self.on_event:
            self.on_event(
                {
                    "type": "reminder.triggered",
                    "reminder": reminder,
                    "message": message,
                }
            )

