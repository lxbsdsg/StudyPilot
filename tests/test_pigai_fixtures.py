"""Offline regression tests for the legacy Pigai page parser."""

from __future__ import annotations

from pathlib import Path

from studypilot.adapters.pigai import PigaiAdapter

FIXTURES = Path(__file__).parent.parent / "server" / "tests" / "fixtures" / "pigai"


def fixture(name: str) -> str:
    return (FIXTURES / name).read_text(encoding="utf-8")


def test_wait_complete_fixture() -> None:
    tasks = PigaiAdapter._parse_wait_complete(fixture("wait-complete.html"))
    assert [task.to_dict() for task in tasks] == [
        {
            "platform": "pigai",
            "course_id": "pigai:all",
            "course_name": "批改网作业",
            "title": "The Value of Teamwork",
            "task_id": "2311479",
            "status": "pending",
            "deadline": "2026-12-01 23:59",
            "url": (
                "https://www.pigai.org/index.php?c=write&a=writeFrom&rid=2311479"
            ),
            "detail": "王老师 · 截止 2026-12-01 23:59",
            "source": "wait-complete",
            "raw": {
                "cells": [
                    "2311479",
                    "The Value of Teamwork",
                    "王老师",
                    "2026级1班",
                    "2026-12-01 23:59",
                    "开始写作",
                ]
            },
        }
    ]


def test_empty_wait_complete_fixture() -> None:
    assert PigaiAdapter._parse_wait_complete(fixture("wait-complete-empty.html")) == []


def test_own_work_fixture_keeps_only_open_items() -> None:
    tasks = PigaiAdapter._parse_own_work(fixture("own-work.html"))
    assert len(tasks) == 1
    assert tasks[0].task_id == "2311480"
    assert tasks[0].status == "pending"
    assert tasks[0].source == "own-work"
    assert tasks[0].url == (
        "https://www.pigai.org/index.php?c=write&a=view&rid=2311480"
    )


def test_essay_fixture_and_unrelated_page() -> None:
    task = PigaiAdapter._parse_essay_page(
        fixture("essay.html"),
        "2311479",
        "https://www.pigai.org/index.php?c=write&a=searchReq&rid=2311479",
    )
    assert task is not None
    assert task.title == "The Value of Teamwork"
    assert task.deadline == "2026-12-01 23:59"
    assert task.detail == "作文号 2311479 · 教师 王老师"
    assert PigaiAdapter._parse_essay_page(
        "<html><body>无相关内容</body></html>", "1"
    ) is None
