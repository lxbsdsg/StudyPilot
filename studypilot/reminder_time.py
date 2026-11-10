"""Helpers for absolute-date and countdown reminder modes."""

from __future__ import annotations

from datetime import datetime, timedelta


REMINDER_MODES = {
    "absolute": "指定日期时间",
    "countdown": "截止前倒计时",
}

COUNTDOWN_PRESETS = {
    "10m": 10,
    "30m": 30,
    "1h": 60,
    "2h": 120,
    "1d": 24 * 60,
    "2d": 48 * 60,
    "3d": 72 * 60,
    "1w": 7 * 24 * 60,
}


def parse_local_datetime(value: str) -> datetime:
    text = (value or "").strip().replace("Z", "+00:00")
    try:
        parsed = datetime.fromisoformat(text)
    except ValueError as exc:
        raise ValueError("提醒时间格式无效") from exc
    if parsed.tzinfo is not None:
        parsed = parsed.astimezone().replace(tzinfo=None)
    return parsed


def countdown_reminder_at(deadline: str | datetime, minutes_before: int) -> datetime:
    if not isinstance(minutes_before, int) or isinstance(minutes_before, bool):
        raise ValueError("倒计时必须是整数分钟")
    if minutes_before < 0:
        raise ValueError("倒计时不能为负数")
    deadline_at = (
        parse_local_datetime(deadline)
        if isinstance(deadline, str)
        else deadline
    )
    remind_at = deadline_at - timedelta(minutes=minutes_before)
    if remind_at <= datetime.now():
        raise ValueError("按该倒计时计算出的提醒时间已经过去")
    return remind_at


def normalize_reminder_mode(
    mode: str | None,
    *,
    remind_at: str | None,
    deadline: str | None,
    minutes_before: int | None,
) -> datetime:
    selected = mode or ("countdown" if minutes_before is not None else "absolute")
    if selected == "absolute":
        if not remind_at:
            raise ValueError("请选择提醒日期和时间")
        value = parse_local_datetime(remind_at)
        if value <= datetime.now():
            raise ValueError("提醒时间必须晚于当前时间")
        return value
    if selected == "countdown":
        if not deadline:
            raise ValueError("该任务没有截止时间，无法设置倒计时提醒")
        if minutes_before is None:
            raise ValueError("请选择截止前多久提醒")
        return countdown_reminder_at(deadline, minutes_before)
    raise ValueError("未知的提醒模式")

