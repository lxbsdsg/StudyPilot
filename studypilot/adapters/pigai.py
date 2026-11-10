"""批改网 adapter backed by a real Chromium login flow.

批改网 encrypts the password in page JavaScript and issues _JUKU_USER after
login. Chromium handles both without hard-coding a moving public key or
reimplementing the page's RSA flow.
"""

from __future__ import annotations

import json
import os
import re
import urllib.parse
from pathlib import Path

from bs4 import BeautifulSoup
from playwright.sync_api import Error as PlaywrightError
from playwright.sync_api import TimeoutError as PlaywrightTimeout
from playwright.sync_api import sync_playwright

from ..models import AccountInfo, Course, RefreshResult, StudyTask
from ..parse_utils import parse_deadline
from .base import PlatformAdapter, PlatformAuthError, PlatformError

BASE = "https://www.pigai.org"
WAIT_COMPLETE = f"{BASE}/index.php?c=write&a=waitComplete&bf=4"
SEARCH = f"{BASE}/index.php"

CHROMIUM_CANDIDATES = (
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
)


def _browser_executable() -> str | None:
    configured = os.environ.get("STUDYPILOT_CHROMIUM", "").strip()
    if configured and Path(configured).exists():
        return configured
    for candidate in CHROMIUM_CANDIDATES:
        if Path(candidate).exists():
            return candidate
    return None


def _cookie_header(cookies: list[dict]) -> str:
    parts = []
    for cookie in cookies:
        domain = str(cookie.get("domain") or "")
        if cookie.get("name") and "pigai.org" in domain:
            parts.append(f"{cookie['name']}={cookie.get('value', '')}")
    return "; ".join(parts)


class PigaiAdapter(PlatformAdapter):
    platform = "pigai"

    def login(self) -> dict:
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
                page.goto(BASE + "/", wait_until="domcontentloaded", timeout=60_000)
                page.fill("#username", self.account["username"])
                page.fill("#password", self.account.get("password", ""))
                page.locator("#ulogin").click()
                page.wait_for_timeout(4_000)
                if page.locator("#username").count():
                    notice = " ".join(page.locator("body").inner_text()[:300].split())
                    raise PlatformAuthError(
                        "批改网登录失败，请检查账号密码"
                        + (f"：{notice}" if notice else "")
                    )
                state = context.storage_state()
                cookies = state.get("cookies") or []
                self.account["session"] = {
                    "version": 1,
                    "cookies": cookies,
                    "uses_chromium_login": True,
                }
                profile = self._profile_from_cookies(cookies)
                context.close()
                browser.close()
        except (PlaywrightError, PlaywrightTimeout) as exc:
            raise PlatformError(f"批改网浏览器登录失败：{exc}") from exc

        self._restore_session()
        return {
            "ok": True,
            "display_name": profile.get("u2", ""),
            "school": profile.get("s", ""),
            "user_id": profile.get("i", ""),
        }

    @staticmethod
    def _profile_from_cookies(cookies: list[dict]) -> dict:
        raw = ""
        for cookie in cookies:
            if cookie.get("name") == "_JUKU_USER":
                raw = str(cookie.get("value") or "")
                break
        if not raw:
            return {}
        try:
            return json.loads(urllib.parse.unquote(raw))
        except ValueError:
            return {}

    def _profile(self) -> dict:
        cookies = (self.account.get("session") or {}).get("cookies") or []
        return self._profile_from_cookies(cookies)

    def refresh(self) -> RefreshResult:
        cookies = (self.account.get("session") or {}).get("cookies") or []
        cookie_header = _cookie_header(cookies)
        headers = {"Cookie": cookie_header} if cookie_header else {}
        wait_response = self.client.get(WAIT_COMPLETE, headers=headers)
        if "用户登录" in wait_response.text and "_JUKU_USER" not in cookie_header:
            raise PlatformAuthError("批改网登录态已失效")

        own_response = self.client.get(
            f"{BASE}/index.php?c=write",
            headers=headers,
        )
        tasks = self._parse_wait_complete(wait_response.text)
        known_ids = {task.task_id for task in tasks}
        for task in self._parse_own_work(own_response.text):
            if task.task_id not in known_ids:
                tasks.append(task)
                known_ids.add(task.task_id)
        profile = self._profile()
        account = AccountInfo(
            platform=self.platform,
            username=self.account["username"],
            display_name=str(profile.get("u2") or self.account["username"]),
            user_id=str(profile.get("i") or ""),
            school=str(profile.get("s") or ""),
            cookies=cookies,
            extra={
                "student_no": profile.get("no", ""),
                "class_id": profile.get("cl", ""),
                "points": profile.get("k", ""),
            },
        )
        courses = [
            Course(
                id="pigai:all",
                name="批改网作业",
                progress=max(0, 100 - min(100, len(tasks) * 10)),
                platform=self.platform,
                raw={"pending": len(tasks)},
            )
        ]
        return RefreshResult(courses=courses, tasks=tasks, account=account)

    def lookup_essay(self, essay_id: str) -> StudyTask:
        essay_id = str(essay_id).strip()
        if not essay_id.isdigit():
            raise ValueError("作文号必须是数字")
        cookies = (self.account.get("session") or {}).get("cookies") or []
        cookie_header = _cookie_header(cookies)
        headers = {"Cookie": cookie_header} if cookie_header else {}
        response = self.client.get(
            SEARCH,
            params={"c": "write", "a": "searchReq", "rid": essay_id},
            headers=headers,
        )
        if "用户登录" in response.text and "_JUKU_USER" not in cookie_header:
            raise PlatformAuthError("批改网登录态已失效")
        task = self._parse_essay_page(response.text, essay_id, str(response.url))
        if task is None:
            raise PlatformError(f"没有找到作文号 {essay_id}，可能不存在或无权访问")
        return task

    @staticmethod
    def _parse_essay_page(
        html: str,
        essay_id: str,
        url: str = "",
    ) -> StudyTask | None:
        soup = BeautifulSoup(html, "html.parser")
        title_input = soup.select_one("#title")
        title = (
            str(title_input.get("value") or "").strip()
            if title_input
            else ""
        )
        if not title:
            title_node = soup.select_one("#timu")
            if title_node:
                text = title_node.get_text(" ", strip=True)
                title = text.replace("题目：", "").strip()
        if not title:
            title = f"第{essay_id}号 作文"
        deadline_text = ""
        for node in soup.select(".contentsW_bar_txt, #contentsW_bar_txt"):
            text = node.get_text(" ", strip=True)
            if "截止时间" in text:
                deadline_text = text
                break
        deadline = parse_deadline(deadline_text)
        request_node = soup.select_one("#v2_request_titlex")
        request_text = (
            request_node.get_text(" ", strip=True) if request_node else ""
        )
        teacher = ""
        match = re.search(r"教师[:：]\s*([^\s]+)", request_text)
        if match:
            teacher = match.group(1)
        has_marker = bool(
            soup.select_one("#timu, #title, #v2_request_titlex")
        )
        if not has_marker:
            return None
        status = "pending"
        if deadline:
            try:
                from datetime import datetime

                if datetime.fromisoformat(deadline) < datetime.now():
                    status = "completed"
            except ValueError:
                pass
        return StudyTask(
            platform="pigai",
            course_id="pigai:all",
            course_name="批改网作业",
            title=title,
            task_id=essay_id,
            status=status,
            deadline=deadline,
            url=url or f"{BASE}/index.php?c=v2&a=write&rid={essay_id}",
            detail=f"作文号 {essay_id}" + (f" · 教师 {teacher}" if teacher else ""),
            source="essay-lookup",
            raw={
                "essay_id": essay_id,
                "request": request_text,
                "deadline_text": deadline_text,
            },
        )

    @staticmethod
    def _parse_wait_complete(html: str) -> list[StudyTask]:
        soup = BeautifulSoup(html, "html.parser")
        container = soup.select_one("#essayList")
        if not container:
            return []
        if "没有待完成" in container.get_text(" ", strip=True):
            return []
        tasks: list[StudyTask] = []
        for row in container.select("tr"):
            cells = [cell.get_text(" ", strip=True) for cell in row.select("th,td")]
            if len(cells) < 5 or not cells[0].isdigit():
                continue
            links = row.select("a[href]")
            href = ""
            for link in links:
                candidate = str(link.get("href") or "")
                if "a=writeFrom" in candidate or "c=write" in candidate:
                    href = candidate
                    break
            if not href and links:
                href = str(links[0].get("href") or "")
            if href.startswith("/"):
                href = BASE + href
            tasks.append(
                StudyTask(
                    platform="pigai",
                    course_id="pigai:all",
                    course_name="批改网作业",
                    title=cells[1],
                    task_id=cells[0],
                    status="pending",
                    deadline=parse_deadline(cells[4]),
                    url=href,
                    detail=f"{cells[2]} · 截止 {cells[4]}",
                    source="wait-complete",
                    raw={"cells": cells},
                )
            )
        return tasks

    @staticmethod
    def _parse_own_work(html: str) -> list[StudyTask]:
        """Read the submitted-work list and keep items whose deadline is still open."""
        soup = BeautifulSoup(html, "html.parser")
        container = soup.select_one("#essayList")
        if not container:
            return []
        tasks: list[StudyTask] = []
        for row in container.select("tr"):
            cells = [cell.get_text(" ", strip=True) for cell in row.select("th,td")]
            if len(cells) < 6:
                continue
            essay_id = cells[0]
            if not essay_id.isdigit():
                continue
            deadline = parse_deadline(cells[4])
            status_text = cells[5]
            if "未截止" not in status_text and deadline is None:
                continue
            href = ""
            for link in row.select("a[href]"):
                candidate = str(link.get("href") or "")
                if "a=view" in candidate or "a=writeFrom" in candidate:
                    href = candidate
                    break
            if href.startswith("/"):
                href = BASE + href
            tasks.append(
                StudyTask(
                    platform="pigai",
                    course_id="pigai:all",
                    course_name="批改网作业",
                    title=cells[1],
                    task_id=essay_id,
                    status=(
                        "pending"
                        if "未截止" in status_text or not status_text
                        else "completed"
                    ),
                    deadline=deadline,
                    url=href,
                    detail=f"{cells[2]} · {status_text or '已提交'}",
                    source="own-work",
                    raw={"cells": cells},
                )
            )
        return tasks

