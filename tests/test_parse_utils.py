from datetime import datetime

from studypilot.parse_utils import parse_deadline


def test_parse_deadline_full_date():
    assert parse_deadline("2026-11-11 23:59") == "2026-11-11 23:59"


def test_parse_deadline_chinese_date():
    assert parse_deadline("截止时间：2026年11月11日 23:59") == "2026-11-11 23:59"


def test_parse_deadline_without_minute():
    assert parse_deadline("2026/12/01 18") == "2026-12-01 18:00"


def test_parse_deadline_invalid():
    assert parse_deadline("未截止") is None

