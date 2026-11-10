"""学习通 adapter based on Doraemon's login and course/homework endpoints."""

from __future__ import annotations

import html
import re
from urllib.parse import parse_qs, urlparse

from bs4 import BeautifulSoup

from ..models import AccountInfo, Course, RefreshResult, StudyTask
from ..parse_utils import nested_dict, parse_deadline
from .base import PlatformAdapter, PlatformAuthError, PlatformError


class ChaoxingAdapter(PlatformAdapter):
    platform = "chaoxing"

    def login(self) -> dict:
        username = self.account["username"]
        password = self.account.get("password", "")
        response = self.client.get(
            "https://passport2-api.chaoxing.com/v11/loginregister",
            params={
                "code": password,
                "cx_xxt_passport": "json",
                "uname": username,
                "loginType": "1",
                "roleSelect": "true",
            },
        )
        data = self.json_dict(response)
        status = data.get("status")
        success = (
            status is True
            or str(status).lower() in {"success", "1", "true"}
            or bool(data.get("_uid"))
        )
        if not success:
            raise PlatformAuthError(str(data.get("msg") or "学习通登录失败"))
        return data

    def refresh(self) -> RefreshResult:
        response = self.client.get(
            "http://mooc1-api.chaoxing.com/mycourse/backclazzdata"
        )
        data = self.json_dict(response)
        raw_channels = data.get("channelList")
        if not isinstance(raw_channels, list):
            raw_channels = data.get("data") if isinstance(data.get("data"), list) else []
        if not raw_channels:
            raise PlatformError("学习通课程列表为空或登录态已失效")

        courses: list[Course] = []
        tasks: list[StudyTask] = []
        for channel in raw_channels:
            channel_data = nested_dict(channel)
            content = nested_dict(channel_data.get("content"))
            course_info = self._course_info(content)
            class_info = self._class_info(content)
            course_id = str(
                course_info.get("id")
                or content.get("courseid")
                or content.get("courseId")
                or course_info.get("id")
                or channel_data.get("courseid")
                or ""
            )
            class_id = str(
                class_info.get("clazzId")
                or class_info.get("classId")
                or class_info.get("id")
                or content.get("clazzid")
                or content.get("clazzId")
                or content.get("classid")
                or channel_data.get("classid")
                or channel_data.get("key")
                or ""
            )
            course_name = str(
                course_info.get("name")
                or channel_data.get("name")
                or content.get("name")
                or class_info.get("clazzName")
                or "未命名课程"
            )
            cpi = str(channel_data.get("cpi") or content.get("cpi") or "")
            if not course_id or not class_id:
                continue
            courses.append(
                Course(
                    id=f"{course_id}:{class_id}",
                    name=course_name,
                    progress=0,
                    platform=self.platform,
                    raw=channel_data,
                )
            )
            try:
                work_enc = self._work_enc(course_id, class_id, cpi)
                homework = self._homework(course_id, class_id, cpi, work_enc)
            except Exception:
                homework = []
            for item in homework:
                status_text = str(item.get("status") or "")
                status = "completed" if "已" in status_text and "未" not in status_text else "pending"
                deadline = parse_deadline(str(item.get("deadline") or ""))
                tasks.append(
                    StudyTask(
                        platform=self.platform,
                        course_id=f"{course_id}:{class_id}",
                        course_name=course_name,
                        title=str(item.get("title") or "未命名作业"),
                        task_id=str(item.get("url") or item.get("title") or ""),
                        status=status,
                        deadline=deadline,
                        url=str(item.get("url") or ""),
                        detail=status_text,
                        source="homework",
                        raw=item,
                    )
                )

        user_name = self.account["username"]
        try:
            profile = self.client.get("http://i.chaoxing.com/base")
            soup = BeautifulSoup(profile.text, "html.parser")
            node = soup.select_one(".user-con")
            if node and node.get_text(strip=True):
                user_name = node.get_text(strip=True)
        except Exception:
            pass

        account = AccountInfo(
            platform=self.platform,
            username=self.account["username"],
            display_name=user_name,
            cookies=self.export_session().get("cookies", []),
        )
        return RefreshResult(courses=courses, tasks=tasks, account=account)

    @staticmethod
    def _course_info(content: dict) -> dict:
        course = nested_dict(content.get("course"))
        data = course.get("data")
        if isinstance(data, list) and data:
            return nested_dict(data[0])
        if course:
            return course
        data = content.get("data")
        if isinstance(data, list) and data:
            return nested_dict(data[0])
        return {}

    @staticmethod
    def _class_info(content: dict) -> dict:
        clazz = content.get("clazz")
        if isinstance(clazz, list) and clazz:
            return nested_dict(clazz[0])
        return nested_dict(content.get("clazz"))

    def _work_enc(self, course_id: str, class_id: str, cpi: str) -> str:
        response = self.client.get(
            "https://mooc1-2.chaoxing.com/mooc-ans/visit/stucoursemiddle",
            params={
                "courseid": course_id,
                "clazzid": class_id,
                "vc": "1",
                "cpi": cpi,
                "ismooc2": "1",
                "v": "2",
            },
        )
        soup = BeautifulSoup(response.text, "html.parser")
        node = soup.find(id="workEnc")
        if node and node.get("value"):
            return str(node["value"])
        match = re.search(r'id=["\']workEnc["\'][^>]*value=["\']([^"\']+)', response.text)
        if match:
            return html.unescape(match.group(1))
        raise PlatformError("学习通作业入口解析失败")

    def _homework(
        self,
        course_id: str,
        class_id: str,
        cpi: str,
        work_enc: str,
    ) -> list[dict]:
        results: list[dict] = []
        page = 1
        page_count = 1
        while page <= page_count and page <= 20:
            response = self.client.get(
                "https://mooc1.chaoxing.com/mooc-ans/mooc2/work/list",
                params={
                    "courseId": course_id,
                    "classId": class_id,
                    "cpi": cpi,
                    "ut": "s",
                    "enc": work_enc,
                    "pageNum": page,
                },
            )
            soup = BeautifulSoup(response.text, "html.parser")
            match = re.search(r"pageNum\s*:\s*(\d+)", response.text)
            if match:
                page_count = int(match.group(1))
            container = soup.select_one(".bottomList")
            if not container:
                break
            for item in container.select("ul > li[data]"):
                url = item.get("data", "")
                title_node = item.select_one(".overHidden2")
                status_node = item.select_one(".status")
                deadline_node = item.select_one(".time")
                results.append(
                    {
                        "url": url,
                        "title": title_node.get_text(" ", strip=True) if title_node else "",
                        "status": status_node.get_text(" ", strip=True) if status_node else "",
                        "deadline": (
                            deadline_node.get_text(" ", strip=True)
                            if deadline_node
                            else ""
                        ),
                    }
                )
            page += 1
        return results

