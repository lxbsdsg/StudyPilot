"""SQLite persistence for accounts, task cache, reminders, and settings."""

from __future__ import annotations

import json
import sqlite3
import threading
import time
import uuid
from pathlib import Path
from typing import Any

from .config import DATABASE_FILE
from .security import protect, unprotect

SCHEMA = """
CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY,
    platform TEXT NOT NULL,
    username TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    secret BLOB,
    display_name TEXT NOT NULL DEFAULT '',
    user_id TEXT NOT NULL DEFAULT '',
    school TEXT NOT NULL DEFAULT '',
    session TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'ready',
    last_error TEXT NOT NULL DEFAULT '',
    last_refresh INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(platform, username)
);

CREATE TABLE IF NOT EXISTS courses (
    account_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    course_id TEXT NOT NULL,
    name TEXT NOT NULL,
    progress INTEGER NOT NULL DEFAULT 0,
    raw TEXT NOT NULL DEFAULT '{}',
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(account_id, course_id)
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    course_id TEXT NOT NULL,
    course_name TEXT NOT NULL,
    title TEXT NOT NULL,
    remote_id TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    deadline TEXT,
    url TEXT NOT NULL DEFAULT '',
    detail TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'task-list',
    raw TEXT NOT NULL DEFAULT '{}',
    first_seen INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(account_id, platform, course_id, remote_id, title)
);

CREATE TABLE IF NOT EXISTS reminders (
    id TEXT PRIMARY KEY,
    account_id TEXT,
    task_id TEXT,
    title TEXT NOT NULL,
    remind_at TEXT NOT NULL,
    repeat_rule TEXT NOT NULL DEFAULT 'once',
    enabled INTEGER NOT NULL DEFAULT 1,
    last_triggered INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS platform_checks (
    platform TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    checked_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tasks_account ON tasks(account_id);
CREATE INDEX IF NOT EXISTS idx_tasks_deadline ON tasks(deadline);
CREATE INDEX IF NOT EXISTS idx_reminders_time ON reminders(remind_at, enabled);
"""


def _json_load(value: str, default: Any) -> Any:
    try:
        return json.loads(value)
    except (TypeError, ValueError):
        return default


class Store:
    def __init__(self, path: Path | str = DATABASE_FILE) -> None:
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.RLock()
        with self._connect() as conn:
            conn.executescript(SCHEMA)

    def _connect(self) -> sqlite3.Connection:
        conn = sqlite3.connect(self.path, timeout=20, check_same_thread=False)
        conn.row_factory = sqlite3.Row
        conn.execute("PRAGMA journal_mode=WAL")
        conn.execute("PRAGMA foreign_keys=ON")
        return conn

    @staticmethod
    def _now() -> int:
        return int(time.time())

    def _account_row(self, row: sqlite3.Row, include_secret: bool = False) -> dict:
        data = {
            "id": row["id"],
            "platform": row["platform"],
            "username": row["username"],
            "label": row["label"],
            "display_name": row["display_name"],
            "user_id": row["user_id"],
            "school": row["school"],
            "session": _json_load(row["session"], {}),
            "status": row["status"],
            "last_error": row["last_error"],
            "last_refresh": row["last_refresh"],
            "created_at": row["created_at"],
            "updated_at": row["updated_at"],
        }
        if include_secret:
            data["password"] = self._decrypt_secret(row["secret"])
        return data

    @staticmethod
    def _decrypt_secret(secret: bytes | None) -> str:
        if not secret:
            return ""
        try:
            return unprotect(bytes(secret)).decode("utf-8")
        except Exception:
            return ""

    def list_accounts(self) -> list[dict]:
        with self._lock, self._connect() as conn:
            rows = conn.execute(
                "SELECT * FROM accounts ORDER BY created_at DESC"
            ).fetchall()
        return [self._account_row(row) for row in rows]

    def get_account(self, account_id: str, include_secret: bool = False) -> dict | None:
        with self._lock, self._connect() as conn:
            row = conn.execute(
                "SELECT * FROM accounts WHERE id = ?", (account_id,)
            ).fetchone()
        return self._account_row(row, include_secret) if row else None

    def add_account(
        self,
        platform: str,
        username: str,
        password: str,
        label: str = "",
    ) -> dict:
        now = self._now()
        account_id = uuid.uuid4().hex
        encrypted = protect(password.encode("utf-8"))
        with self._lock, self._connect() as conn:
            existing = conn.execute(
                "SELECT id FROM accounts WHERE platform = ? AND username = ?",
                (platform, username),
            ).fetchone()
            if existing:
                account_id = existing["id"]
                conn.execute(
                    """
                    UPDATE accounts
                       SET label = ?, secret = ?, status = 'ready', last_error = '',
                           updated_at = ?
                     WHERE id = ?
                    """,
                    (label, encrypted, now, account_id),
                )
            else:
                conn.execute(
                    """
                    INSERT INTO accounts
                        (id, platform, username, label, secret, created_at, updated_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        account_id,
                        platform,
                        username,
                        label,
                        encrypted,
                        now,
                        now,
                    ),
                )
            conn.commit()
        return self.get_account(account_id) or {}

    def update_account(self, account_id: str, **fields: Any) -> bool:
        allowed = {
            "label",
            "display_name",
            "user_id",
            "school",
            "session",
            "status",
            "last_error",
            "last_refresh",
        }
        updates = {key: value for key, value in fields.items() if key in allowed}
        if not updates:
            return False
        updates["updated_at"] = self._now()
        if "session" in updates and not isinstance(updates["session"], str):
            updates["session"] = json.dumps(updates["session"], ensure_ascii=False)
        columns = ", ".join(f"{key} = ?" for key in updates)
        values = list(updates.values()) + [account_id]
        with self._lock, self._connect() as conn:
            cursor = conn.execute(
                f"UPDATE accounts SET {columns} WHERE id = ?", values
            )
            conn.commit()
        return cursor.rowcount > 0

    def delete_account(self, account_id: str) -> bool:
        with self._lock, self._connect() as conn:
            conn.execute("DELETE FROM tasks WHERE account_id = ?", (account_id,))
            conn.execute("DELETE FROM courses WHERE account_id = ?", (account_id,))
            conn.execute("DELETE FROM reminders WHERE account_id = ?", (account_id,))
            cursor = conn.execute("DELETE FROM accounts WHERE id = ?", (account_id,))
            conn.commit()
        return cursor.rowcount > 0

    def replace_account_data(
        self,
        account_id: str,
        courses: list[dict],
        tasks: list[dict],
    ) -> None:
        now = self._now()
        platform = (courses[0].get("platform") if courses else "") or (
            tasks[0].get("platform") if tasks else ""
        )
        with self._lock, self._connect() as conn:
            conn.execute("DELETE FROM courses WHERE account_id = ?", (account_id,))
            for course in courses:
                conn.execute(
                    """
                    INSERT INTO courses
                        (account_id, platform, course_id, name, progress, raw, updated_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        account_id,
                        course.get("platform") or platform,
                        str(course.get("id", "")),
                        course.get("name", ""),
                        int(course.get("progress") or 0),
                        json.dumps(course.get("raw") or {}, ensure_ascii=False),
                        now,
                    ),
                )
            previous = {
                row["task_key"]: row["id"]
                for row in conn.execute(
                    """
                    SELECT id, account_id || '|' || platform || '|' || course_id || '|'
                           || remote_id || '|' || title AS task_key
                      FROM tasks WHERE account_id = ?
                    """,
                    (account_id,),
                ).fetchall()
            }
            current_keys: set[str] = set()
            for task in tasks:
                key = "|".join(
                    [
                        account_id,
                        task.get("platform", ""),
                        str(task.get("course_id", "")),
                        str(task.get("task_id", "")),
                        str(task.get("title", "")),
                    ]
                )
                current_keys.add(key)
                task_id = previous.get(key) or uuid.uuid4().hex
                conn.execute(
                    """
                    INSERT INTO tasks
                        (id, account_id, platform, course_id, course_name, title,
                         remote_id, status, deadline, url, detail, source, raw,
                         first_seen, updated_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(account_id, platform, course_id, remote_id, title)
                    DO UPDATE SET
                        course_name = excluded.course_name,
                        status = excluded.status,
                        deadline = excluded.deadline,
                        url = excluded.url,
                        detail = excluded.detail,
                        source = excluded.source,
                        raw = excluded.raw,
                        updated_at = excluded.updated_at
                    """,
                    (
                        task_id,
                        account_id,
                        task.get("platform", platform),
                        str(task.get("course_id", "")),
                        task.get("course_name", ""),
                        task.get("title", ""),
                        str(task.get("task_id", "")),
                        task.get("status", "pending"),
                        task.get("deadline"),
                        task.get("url", ""),
                        task.get("detail", ""),
                        task.get("source", "task-list"),
                        json.dumps(task.get("raw") or {}, ensure_ascii=False),
                        now,
                        now,
                    ),
                )
            stale = set(previous) - current_keys
            for key in stale:
                conn.execute("DELETE FROM tasks WHERE id = ?", (previous[key],))
            conn.commit()

    def list_tasks(
        self,
        account_id: str | None = None,
        status: str | None = None,
        platform: str | None = None,
    ) -> list[dict]:
        query = "SELECT * FROM tasks WHERE 1=1"
        params: list[Any] = []
        if account_id:
            query += " AND account_id = ?"
            params.append(account_id)
        if status:
            query += " AND status = ?"
            params.append(status)
        if platform:
            query += " AND platform = ?"
            params.append(platform)
        query += """
            ORDER BY
                CASE WHEN deadline IS NULL OR deadline = '' THEN 1 ELSE 0 END,
                deadline ASC,
                updated_at DESC
        """
        with self._lock, self._connect() as conn:
            rows = conn.execute(query, params).fetchall()
        return [
            {
                "id": row["id"],
                "account_id": row["account_id"],
                "platform": row["platform"],
                "course_id": row["course_id"],
                "course_name": row["course_name"],
                "title": row["title"],
                "task_id": row["remote_id"],
                "status": row["status"],
                "deadline": row["deadline"],
                "url": row["url"],
                "detail": row["detail"],
                "source": row["source"],
                "raw": _json_load(row["raw"], {}),
                "first_seen": row["first_seen"],
                "updated_at": row["updated_at"],
            }
            for row in rows
        ]

    def update_task_status(self, task_id: str, status: str) -> bool:
        with self._lock, self._connect() as conn:
            cursor = conn.execute(
                "UPDATE tasks SET status = ?, updated_at = ? WHERE id = ?",
                (status, self._now(), task_id),
            )
            conn.commit()
        return cursor.rowcount > 0

    def list_courses(self, account_id: str | None = None) -> list[dict]:
        query = "SELECT * FROM courses"
        params: list[Any] = []
        if account_id:
            query += " WHERE account_id = ?"
            params.append(account_id)
        query += " ORDER BY progress ASC, name ASC"
        with self._lock, self._connect() as conn:
            rows = conn.execute(query, params).fetchall()
        return [
            {
                "account_id": row["account_id"],
                "platform": row["platform"],
                "id": row["course_id"],
                "name": row["name"],
                "progress": row["progress"],
                "raw": _json_load(row["raw"], {}),
                "updated_at": row["updated_at"],
            }
            for row in rows
        ]

    def add_reminder(
        self,
        title: str,
        remind_at: str,
        account_id: str | None = None,
        task_id: str | None = None,
        repeat_rule: str = "once",
    ) -> dict:
        reminder_id = uuid.uuid4().hex
        with self._lock, self._connect() as conn:
            conn.execute(
                """
                INSERT INTO reminders
                    (id, account_id, task_id, title, remind_at, repeat_rule,
                     enabled, created_at)
                VALUES (?, ?, ?, ?, ?, ?, 1, ?)
                """,
                (
                    reminder_id,
                    account_id,
                    task_id,
                    title,
                    remind_at,
                    repeat_rule,
                    self._now(),
                ),
            )
            conn.commit()
        return self.get_reminder(reminder_id) or {}

    def get_reminder(self, reminder_id: str) -> dict | None:
        with self._lock, self._connect() as conn:
            row = conn.execute(
                "SELECT * FROM reminders WHERE id = ?", (reminder_id,)
            ).fetchone()
        return dict(row) if row else None

    def list_reminders(self) -> list[dict]:
        with self._lock, self._connect() as conn:
            rows = conn.execute(
                """
                SELECT r.*, t.title AS task_title, a.platform, a.label AS account_label
                  FROM reminders r
                  LEFT JOIN tasks t ON t.id = r.task_id
                  LEFT JOIN accounts a ON a.id = r.account_id
                 ORDER BY r.enabled DESC, r.remind_at ASC
                """
            ).fetchall()
        return [
            {
                **dict(row),
                "enabled": bool(row["enabled"]),
            }
            for row in rows
        ]

    def update_reminder(self, reminder_id: str, **fields: Any) -> bool:
        allowed = {"title", "remind_at", "repeat_rule", "enabled"}
        updates = {key: value for key, value in fields.items() if key in allowed}
        if "enabled" in updates:
            updates["enabled"] = int(bool(updates["enabled"]))
        if not updates:
            return False
        columns = ", ".join(f"{key} = ?" for key in updates)
        values = list(updates.values()) + [reminder_id]
        with self._lock, self._connect() as conn:
            cursor = conn.execute(
                f"UPDATE reminders SET {columns} WHERE id = ?", values
            )
            conn.commit()
        return cursor.rowcount > 0

    def mark_reminder_triggered(self, reminder_id: str, disable: bool = False) -> None:
        with self._lock, self._connect() as conn:
            conn.execute(
                "UPDATE reminders SET last_triggered = ?, enabled = ? WHERE id = ?",
                (self._now(), 0 if disable else 1, reminder_id),
            )
            conn.commit()

    def delete_reminder(self, reminder_id: str) -> bool:
        with self._lock, self._connect() as conn:
            cursor = conn.execute("DELETE FROM reminders WHERE id = ?", (reminder_id,))
            conn.commit()
        return cursor.rowcount > 0

    def get_setting(self, key: str, default: Any = None) -> Any:
        with self._lock, self._connect() as conn:
            row = conn.execute(
                "SELECT value FROM settings WHERE key = ?", (key,)
            ).fetchone()
        return _json_load(row["value"], default) if row else default

    def set_setting(self, key: str, value: Any) -> None:
        with self._lock, self._connect() as conn:
            conn.execute(
                """
                INSERT INTO settings(key, value) VALUES(?, ?)
                ON CONFLICT(key) DO UPDATE SET value = excluded.value
                """,
                (key, json.dumps(value, ensure_ascii=False)),
            )
            conn.commit()

    def set_platform_check(self, platform: str, status: str, note: str = "") -> None:
        with self._lock, self._connect() as conn:
            conn.execute(
                """
                INSERT INTO platform_checks(platform, status, note, checked_at)
                VALUES(?, ?, ?, ?)
                ON CONFLICT(platform) DO UPDATE SET
                    status = excluded.status,
                    note = excluded.note,
                    checked_at = excluded.checked_at
                """,
                (platform, status, note, self._now()),
            )
            conn.commit()

    def get_platform_checks(self) -> dict[str, dict]:
        with self._lock, self._connect() as conn:
            rows = conn.execute(
                "SELECT * FROM platform_checks ORDER BY platform"
            ).fetchall()
        return {
            row["platform"]: {
                "status": row["status"],
                "note": row["note"],
                "checked_at": row["checked_at"],
            }
            for row in rows
        }

