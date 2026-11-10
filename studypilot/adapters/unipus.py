"""U校园 read-only adapter based on the existing Unipus Helper contracts."""

from __future__ import annotations

import time
from typing import Any

import httpx

from ..models import AccountInfo, Course, RefreshResult, StudyTask
from ..parse_utils import as_int, nested_dict
from .base import PlatformAdapter, PlatformAuthError, PlatformError

MAIN_HOST = "uai.unipus.cn"
SSO_HOST = "sso.unipus.cn"
CONTENT_HOST = "ucontent.unipus.cn"


class UnipusAdapter(PlatformAdapter):
    platform = "unipus"

    def login(self) -> dict:
        response = self.client.post(
            f"https://{SSO_HOST}/sso/0.1/sso/login",
            json={
                "username": self.account["username"],
                "password": self.account.get("password", ""),
                "remember": True,
                "agreement": True,
                "service": "https://uai.unipus.cn/home",
            },
        )
        data = self.json_dict(response)
        result = data.get("rs")
        if not isinstance(result, dict):
            raise PlatformAuthError(str(data.get("msg") or "账号或密码错误"))
        self._login_result = result
        return result

    def _get(self, host: str, path: str) -> dict:
        response = self.client.get(f"https://{host}/{path.lstrip('/')}")
        self.ensure_ok(response)
        return self.json_dict(response)

    def _user_info(self) -> dict:
        try:
            data = self._get(MAIN_HOST, "api/account/user/info")
            return nested_dict(data.get("value"))
        except Exception:
            return {}

    def refresh(self) -> RefreshResult:
        data = self._get(MAIN_HOST, "api/cmgt/course/getCourseListByStudent")
        value = nested_dict(data.get("value"))
        raw_courses = value.get("courseList")
        if not isinstance(raw_courses, list):
            raise PlatformError("U校园课程列表返回异常")

        courses: list[Course] = []
        tasks: list[StudyTask] = []
        for raw_course in raw_courses:
            course_data = nested_dict(raw_course)
            course_id = str(course_data.get("id") or "")
            course_name = str(course_data.get("name") or "未命名课程")
            resources = course_data.get("courseResourceList")
            resources = resources if isinstance(resources, list) else []
            finish = 0
            total = 0
            for raw_resource in resources:
                resource = nested_dict(raw_resource)
                finish += as_int(resource.get("finishPointNum"))
                total += as_int(resource.get("totalPointNum"))
            progress = int(round(finish / total * 100)) if total else 0
            courses.append(
                Course(
                    id=course_id,
                    name=course_name,
                    progress=progress,
                    platform=self.platform,
                    raw=course_data,
                )
            )
            start_time = as_int(course_data.get("startTime"))
            end_time = as_int(course_data.get("endTime"))
            if end_time and end_time >= int(time.time() * 1000) and progress < 100:
                deadline = time.strftime(
                    "%Y-%m-%d %H:%M", time.localtime(end_time / 1000)
                )
                tasks.append(
                    StudyTask(
                        platform=self.platform,
                        course_id=course_id,
                        course_name=course_name,
                        title=f"{course_name} 学习任务",
                        task_id=f"course:{course_id}",
                        status="pending" if progress < 100 else "completed",
                        deadline=deadline,
                        url="https://uai.unipus.cn/",
                        detail=f"当前进度 {progress}%",
                        source="course",
                    )
                )

            for raw_resource in resources:
                resource = nested_dict(raw_resource)
                resource_id = str(resource.get("resourceId") or "")
                resource_name = str(resource.get("name") or "学习资源")
                resource_finish = as_int(resource.get("finishPointNum"))
                resource_total = as_int(resource.get("totalPointNum"))
                if resource_total and resource_finish < resource_total:
                    resource_progress = int(
                        round(resource_finish / resource_total * 100)
                    )
                    tasks.append(
                        StudyTask(
                            platform=self.platform,
                            course_id=course_id,
                            course_name=course_name,
                            title=resource_name,
                            task_id=resource_id or resource_name,
                            status="pending",
                            deadline=(
                                time.strftime(
                                    "%Y-%m-%d %H:%M",
                                    time.localtime(end_time / 1000),
                                )
                                if end_time
                                else None
                            ),
                            url="https://uai.unipus.cn/",
                            detail=f"完成 {resource_finish}/{resource_total}",
                            source="resource",
                            raw=resource,
                        )
                    )

        info = self._user_info()
        account = AccountInfo(
            platform=self.platform,
            username=self.account["username"],
            display_name=str(
                info.get("name") or info.get("nickName") or self.account["username"]
            ),
            user_id=str(info.get("id") or info.get("userId") or ""),
            school=str(info.get("schoolName") or info.get("school") or ""),
            cookies=self.export_session().get("cookies", []),
        )
        return RefreshResult(courses=courses, tasks=tasks, account=account)

