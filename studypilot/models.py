"""Domain models shared by adapters and the API."""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Any


@dataclass
class Course:
    id: str
    name: str
    progress: int = 0
    platform: str = ""
    raw: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class StudyTask:
    platform: str
    course_id: str
    course_name: str
    title: str
    task_id: str = ""
    status: str = "pending"
    deadline: str | None = None
    url: str = ""
    detail: str = ""
    source: str = "task-list"
    raw: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class AccountInfo:
    platform: str
    username: str
    display_name: str = ""
    user_id: str = ""
    school: str = ""
    cookies: list[dict] = field(default_factory=list)
    extra: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class RefreshResult:
    courses: list[Course] = field(default_factory=list)
    tasks: list[StudyTask] = field(default_factory=list)
    account: AccountInfo | None = None
    warning: str = ""

