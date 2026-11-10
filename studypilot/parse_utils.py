"""Small parsers shared by platform adapters."""

from __future__ import annotations

import json
import re
from datetime import datetime, timedelta
from typing import Any


def as_int(value: Any, default: int = 0) -> int:
    try:
        text = str(value).strip()
        return int(float(text)) if text else default
    except (TypeError, ValueError):
        return default


def nested_dict(value: Any) -> dict:
    if isinstance(value, dict):
        return value
    if isinstance(value, str):
        try:
            parsed = json.loads(value)
            return parsed if isinstance(parsed, dict) else {}
        except ValueError:
            return {}
    return {}


def parse_deadline(text: str) -> str | None:
    raw = (text or "").strip()
    if not raw:
        return None
    raw = raw.replace("截止时间", "").replace("截止", "").strip(" ：:")
    match = re.search(
        r"(20\d{2})[-/.年](\d{1,2})[-/.月](\d{1,2})"
        r"(?:[日号]?\s*(\d{1,2})(?:[:：](\d{2}))?)?",
        raw,
    )
    if match:
        year, month, day, hour, minute = match.groups()
        try:
            value = datetime(
                int(year),
                int(month),
                int(day),
                int(hour or 0),
                int(minute or 0),
            )
            return value.strftime("%Y-%m-%d %H:%M")
        except ValueError:
            return None
    match = re.search(r"(\d{1,2})[-/.月](\d{1,2}).*?(\d{1,2})[:：](\d{2})?", raw)
    if match:
        month, day, hour, minute = match.groups()
        now = datetime.now()
        try:
            value = datetime(
                now.year,
                int(month),
                int(day),
                int(hour),
                int(minute or 0),
            )
            if value < now - timedelta(days=180):
                value = value.replace(year=value.year + 1)
            return value.strftime("%Y-%m-%d %H:%M")
        except ValueError:
            return None
    return None

