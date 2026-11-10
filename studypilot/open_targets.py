"""Cross-platform desktop and mobile open targets."""

from __future__ import annotations

from .adapters.pigai import ANDROID_APP, IOS_APP, MOBILE_PAGE

MOBILE_MARKERS = ("android", "iphone", "ipad", "ipod", "mobile")

PLATFORM_TARGETS = {
    "pigai": {
        "web": "https://www.pigai.org/",
        "mobile": MOBILE_PAGE,
        "android": ANDROID_APP,
        "ios": IOS_APP,
    },
    "unipus": {
        "web": "https://uai.unipus.cn/",
        "mobile": "https://uai.unipus.cn/",
        "android": "",
        "ios": "",
    },
    "welearn": {
        "web": "https://welearn.sflep.com/2019/student/index.aspx",
        "mobile": "https://welearn.sflep.com/2019/student/index.aspx",
        "android": "",
        "ios": "",
    },
    "chaoxing": {
        "web": "https://mooc1.chaoxing.com/",
        "mobile": "https://mooc1.chaoxing.com/",
        "android": "",
        "ios": "",
    },
}


def is_mobile_user_agent(user_agent: str) -> bool:
    text = (user_agent or "").lower()
    return any(marker in text for marker in MOBILE_MARKERS)


def desktop_url(platform: str, task_url: str = "") -> str:
    if task_url:
        return task_url
    return PLATFORM_TARGETS.get(platform, {}).get("web", "")


def mobile_targets(platform: str, task_url: str = "") -> dict:
    configured = PLATFORM_TARGETS.get(platform, {})
    return {
        "kind": "mobile",
        "platform": platform,
        "web_url": configured.get("mobile") or desktop_url(platform, task_url),
        "desktop_url": desktop_url(platform, task_url),
        "android_url": configured.get("android", ""),
        "ios_url": configured.get("ios", ""),
    }

