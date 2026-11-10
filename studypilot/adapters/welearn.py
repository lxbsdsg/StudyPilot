"""WELearn read-only adapter.

The login flow follows the existing Welearn_helper implementation. Refreshing
reads course progress and visible unit completion only.
"""

from __future__ import annotations

import base64
import re
import time
from pathlib import Path

from playwright.sync_api import Error as PlaywrightError
from playwright.sync_api import TimeoutError as PlaywrightTimeout
from playwright.sync_api import sync_playwright

from ..models import AccountInfo, Course, RefreshResult, StudyTask
from .base import PlatformAdapter, PlatformAuthError, PlatformError

BASE = "https://welearn.sflep.com"

CHROMIUM_CANDIDATES = (
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
)


def _browser_executable() -> str | None:
    for candidate in CHROMIUM_CANDIDATES:
        if Path(candidate).exists():
            return candidate
    return None


def _cipher(password: str) -> tuple[str, int]:
    raw = password.encode("utf-8")
    now = int(round(time.time() * 1000))
    value = (now >> 16) & 0xFF
    for byte in raw:
        value ^= byte
    timestamp = int((now / 100) * 100 + value % 100)
    payload = f"{timestamp}*{raw.hex()}".encode("utf-8")
    return base64.b64encode(payload).decode("ascii"), timestamp


class WelearnAdapter(PlatformAdapter):
    platform = "welearn"

    def login(self) -> dict:
        if self._login_with_chromium():
            return {"ok": True, "method": "chromium"}
        return self._login_with_http()

    def _login_with_chromium(self) -> bool:
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(
                    headless=True,
                    executable_path=_browser_executable(),
                )
                context = browser.new_context(
                    locale="zh-CN",
                    user_agent=(
                        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
                        "AppleWebKit/537.36 (KHTML, like Gecko) "
                        "Chrome/131.0.0.0 Safari/537.36"
                    ),
                )
                page = context.new_page()
                page.goto(
                    BASE + "/user/prelogin.aspx"
                    "?loginret=http://welearn.sflep.com/user/loginredirect.aspx",
                    wait_until="domcontentloaded",
                    timeout=60_000,
                )
                page.wait_for_timeout(3_000)
                for selector in (
                    'input[name="account"]',
                    'input[name="username"]',
                    'input[type="text"]',
                    "#account",
                    "#username",
                ):
                    locator = page.locator(selector)
                    if locator.count():
                        locator.first.fill(self.account["username"])
                        break
                password_selectors = (
                    'input[name="pwd"]',
                    'input[name="password"]',
                    'input[type="password"]',
                    "#password",
                    "#pwd",
                )
                password_filled = False
                for selector in password_selectors:
                    locator = page.locator(selector)
                    if locator.count():
                        locator.first.fill(self.account.get("password", ""))
                        password_filled = True
                        break
                if not password_filled:
                    return False
                for selector in (
                    'button[type="submit"]',
                    'input[type="submit"]',
                    'button:has-text("登录")',
                    'a:has-text("登录")',
                    ".login-btn",
                ):
                    locator = page.locator(selector)
                    if locator.count():
                        locator.first.click()
                        break
                page.wait_for_timeout(5_000)
                if page.locator('input[type="password"]').count():
                    return False
                state = context.storage_state()
                cookies = state.get("cookies") or []
                if not cookies:
                    return False
                self.account["session"] = {
                    "version": 1,
                    "cookies": cookies,
                    "uses_chromium_login": True,
                }
                context.close()
                browser.close()
            self._restore_session()
            return True
        except (PlaywrightError, PlaywrightTimeout):
            return False

    def _login_with_http(self) -> dict:
        response = self.client.get(
            f"{BASE}/user/prelogin.aspx"
            "?loginret=http://welearn.sflep.com/user/loginredirect.aspx"
        )
        location = response.headers.get("Location", "")
        match_code = re.search(r"[?&]code_challenge=([^&]+)", location)
        match_state = re.search(r"[?&]state=([^&]+)", location)
        if not match_code or not match_state:
            raise PlatformAuthError("WELearn 登录跳转解析失败")
        redirect = (
            "/connect/authorize/callback?client_id=welearn_web"
            "&redirect_uri=https%3A%2F%2Fwelearn.sflep.com%2Fsignin-sflep"
            "&response_type=code&scope=openid%20profile%20email%20phone%20address"
            f"&code_challenge={match_code.group(1)}"
            "&code_challenge_method=S256"
            f"&state={match_state.group(1)}"
            "&x-client-SKU=ID_NET472&x-client-ver=6.32.1.0"
        )
        encrypted, timestamp = _cipher(self.account.get("password", ""))
        response = self.client.post(
            "https://sso.sflep.com/idsvr/account/login",
            data={
                "rturl": redirect,
                "account": self.account["username"],
                "pwd": encrypted,
                "ts": str(timestamp),
            },
        )
        data = self.json_dict(response)
        code = data.get("code", -1)
        if code == 1:
            raise PlatformAuthError("WELearn 账号或密码错误")
        if code != 0:
            raise PlatformAuthError("WELearn 登录失败或需要额外验证")
        next_url = "https://sso.sflep.com/idsvr" + str(data.get("data") or "")
        for _ in range(8):
            response = self.client.get(next_url)
            if response.status_code not in (301, 302, 303, 307, 308):
                break
            next_url = response.headers.get("Location", next_url)
            if next_url.startswith("/"):
                next_url = "https://sso.sflep.com" + next_url
        return data

    def refresh(self) -> RefreshResult:
        response = self.client.get(
            f"{BASE}/ajax/authCourse.aspx?action=gmc",
            headers={"Referer": f"{BASE}/2019/student/index.aspx"},
        )
        data = self.json_dict(response)
        raw_courses = data.get("clist")
        if not isinstance(raw_courses, list) or not raw_courses:
            raise PlatformError("WELearn 没有返回课程，登录态可能已失效")

        courses: list[Course] = []
        tasks: list[StudyTask] = []
        for raw in raw_courses:
            if not isinstance(raw, dict):
                continue
            cid = str(raw.get("cid") or "")
            name = str(raw.get("name") or "未命名课程")
            progress = int(raw.get("per") or 0)
            courses.append(
                Course(
                    id=cid,
                    name=name,
                    progress=progress,
                    platform=self.platform,
                    raw=raw,
                )
            )
            if progress >= 100:
                continue
            try:
                uid, class_id = self._course_ids(cid)
                units = self._units(cid, uid)
            except Exception:
                units = []
            for index, unit in enumerate(units, 1):
                if not isinstance(unit, dict):
                    continue
                if str(unit.get("visible", "true")).lower() == "false":
                    continue
                unit_name = " ".join(
                    filter(
                        None,
                        [str(unit.get("unitname") or ""), str(unit.get("name") or "")],
                    )
                )
                tasks.append(
                    StudyTask(
                        platform=self.platform,
                        course_id=cid,
                        course_name=name,
                        title=unit_name or f"单元 {index}",
                        task_id=f"{cid}:{index}",
                        status="pending",
                        url=f"{BASE}/2019/student/course_info.aspx?cid={cid}",
                        detail=f"课程进度 {progress}%",
                        source="unit",
                        raw=unit,
                    )
                )
        account = AccountInfo(
            platform=self.platform,
            username=self.account["username"],
            display_name=self.account["username"],
            cookies=self.export_session().get("cookies", []),
        )
        return RefreshResult(courses=courses, tasks=tasks, account=account)

    def _course_ids(self, cid: str) -> tuple[str, str]:
        response = self.client.get(f"{BASE}/student/course_info.aspx?cid={cid}")
        uid_match = re.search(r'"uid":(.*?),', response.text)
        class_match = re.search(r'"classid":"(.*?)"', response.text)
        if not uid_match or not class_match:
            raise PlatformError(f"WELearn 课程 {cid} 身份信息解析失败")
        return uid_match.group(1), class_match.group(1)

    def _units(self, cid: str, uid: str) -> list[dict]:
        response = self.client.get(
            f"{BASE}/ajax/StudyStat.aspx",
            params={"action": "courseunits", "cid": cid, "uid": uid},
            headers={"Referer": f"{BASE}/2019/student/course_info.aspx"},
        )
        data = self.json_dict(response)
        value = data.get("info")
        return value if isinstance(value, list) else []

