from datetime import datetime, timedelta

import pytest

from studypilot.reminder_time import (
    countdown_reminder_at,
    normalize_reminder_mode,
)


def test_countdown_reminder_at():
    deadline = datetime(2030, 1, 2, 20, 0)
    assert countdown_reminder_at(deadline, 60) == datetime(2030, 1, 2, 19, 0)


def test_countdown_requires_future_result():
    with pytest.raises(ValueError):
        countdown_reminder_at(datetime.now() - timedelta(minutes=1), 60)


def test_normalize_absolute_mode():
    value = (datetime.now() + timedelta(days=1)).strftime("%Y-%m-%dT%H:%M")
    assert normalize_reminder_mode(
        "absolute",
        remind_at=value,
        deadline=None,
        minutes_before=None,
    ).strftime("%Y-%m-%dT%H:%M") == value


def test_normalize_countdown_mode():
    deadline = (datetime.now() + timedelta(days=2)).strftime("%Y-%m-%dT%H:%M")
    value = normalize_reminder_mode(
        "countdown",
        remind_at=None,
        deadline=deadline,
        minutes_before=60,
    )
    assert value < datetime.fromisoformat(deadline)

