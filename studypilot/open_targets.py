"""Cross-platform desktop and mobile open targets."""

from __future__ import annotations

MOBILE_MARKERS = ("android", "iphone", "ipad", "ipod", "mobile")

PLATFORM_TARGETS = {
    "pigai": {
        "web": "https://www.pigai.org/",
        "mobile": "https://www.pigai.org/zt/mobile/",
    },
    "unipus": {
        "web": "https://uai.unipus.cn/",
        "mobile": "https://uai.unipus.cn/",
    },
    "welearn": {
        "web": "https://welearn.sflep.com/2019/student/index.aspx",
        "mobile": "https://welearn.sflep.com/2019/student/index.aspx",
    },
    "chaoxing": {
        "web": "https://mooc1.chaoxing.com/",
        "mobile": "https://mooc1.chaoxing.com/",
    },
}


def is_mobile_user_agent(user_agent: str) -> bool:
    text = (user_agent or "").lower()
    return any(marker in text for marker in MOBILE_MARKERS)


def desktop_url(platform: str, task_url: str = "") -> str:
    """Return the platform homepage; task URLs are deliberately ignored."""
    return PLATFORM_TARGETS.get(platform, {}).get("web", "")


def mobile_targets(platform: str, task_url: str = "") -> dict:
    """Return only the platform's official mobile website."""
    configured = PLATFORM_TARGETS.get(platform, {})
    return {
        "kind": "mobile",
        "platform": platform,
        "web_url": configured.get("mobile") or configured.get("web", ""),
        "desktop_url": desktop_url(platform, task_url),
    }

