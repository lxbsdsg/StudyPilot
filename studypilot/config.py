"""Application-wide paths and defaults."""

from __future__ import annotations

import os
from pathlib import Path

APP_NAME = "StudyPilot"
VERSION = "0.1.0"
HOST = "127.0.0.1"
DEFAULT_PORT = 17890
PORT_RANGE = range(17890, 17930)

USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 " \
             "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"

DATA_DIR = Path(
    os.environ.get("STUDYPILOT_DATA_DIR", Path.home() / ".studypilot")
).resolve()
DATABASE_FILE = DATA_DIR / "studypilot.db"
SECRET_FILE = DATA_DIR / "secret.key"

PLATFORM_LABELS = {
    "unipus": "U校园",
    "welearn": "WELearn",
    "chaoxing": "学习通",
    "pigai": "批改网",
}

