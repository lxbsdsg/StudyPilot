"""Desktop notifications with safe handling of user-provided text."""

from __future__ import annotations

import base64
import logging
import os
import subprocess
import sys

logger = logging.getLogger(__name__)


def _powershell_literal(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def _windows_toast(title: str, message: str) -> None:
    script = (
        "[Windows.UI.Notifications.ToastNotificationManager, "
        "Windows.UI.Notifications, ContentType=WindowsRuntime] > $null;"
        "$title = " + _powershell_literal(title) + ";"
        "$message = " + _powershell_literal(message) + ";"
        "$template = [Windows.UI.Notifications.ToastNotificationManager]"
        "::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]"
        "::ToastText02);"
        "$texts = $template.GetElementsByTagName('text');"
        "if (-not $texts.Item(0)) { return };"
        "$texts.Item(0).AppendChild($template.CreateTextNode($title)) > $null;"
        "if ($texts.Item(1)) {"
        "  $texts.Item(1).AppendChild($template.CreateTextNode($message)) > $null"
        "};"
        "$toast = [Windows.UI.Notifications.ToastNotification]::new($template);"
        "[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier("
        "'StudyPilot').Show($toast);"
    )
    command = base64.b64encode(script.encode("utf-16-le")).decode("ascii")
    subprocess.Popen(
        [
            "powershell",
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-EncodedCommand",
            command,
        ],
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )


def notify(title: str, message: str) -> None:
    if os.environ.get("STUDYPILOT_DISABLE_NOTIFICATIONS") == "1":
        logger.info("REMINDER %s: %s", title, message)
        return
    if sys.platform == "win32":
        try:
            _windows_toast(title, message)
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
