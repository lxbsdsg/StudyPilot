"""Adapter contract for supported study platforms."""

from __future__ import annotations

import json
from abc import ABC, abstractmethod
from typing import Any

import httpx

from ..config import USER_AGENT
from ..models import RefreshResult


class PlatformError(RuntimeError):
    """Base platform failure."""


class PlatformAuthError(PlatformError):
    """Login or session failure."""


class PlatformAdapter(ABC):
    platform = ""

    def __init__(self, account: dict[str, Any]) -> None:
        self.account = account
        self.client = httpx.Client(
            follow_redirects=True,
            timeout=30.0,
            headers={
                "User-Agent": USER_AGENT,
                "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
            },
        )
        self._restore_session()

    def close(self) -> None:
        self.client.close()

    def _restore_session(self) -> None:
        session = self.account.get("session") or {}
        cookies = session.get("cookies") if isinstance(session, dict) else None
        if not isinstance(cookies, list):
            return
        for cookie in cookies:
            if not isinstance(cookie, dict) or not cookie.get("name"):
                continue
            self.client.cookies.set(
                cookie["name"],
                cookie.get("value", ""),
                domain=cookie.get("domain"),
                path=cookie.get("path", "/"),
            )

    def export_session(self) -> dict:
        return {
            "version": 1,
            "cookies": [
                {
                    "name": cookie.name,
                    "value": cookie.value,
                    "domain": cookie.domain,
                    "path": cookie.path,
                }
                for cookie in self.client.cookies.jar
            ],
        }

    @abstractmethod
    def login(self) -> dict:
        """Validate credentials and return account metadata."""

    @abstractmethod
    def refresh(self) -> RefreshResult:
        """Fetch courses and outstanding tasks without mutating them."""

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

    @staticmethod
    def ensure_ok(response: httpx.Response) -> httpx.Response:
        if response.status_code >= 400:
            raise PlatformError(f"HTTP {response.status_code}: {response.reason_phrase}")
        return response

    @staticmethod
    def json_dict(response: httpx.Response) -> dict:
        try:
            data = response.json()
        except ValueError as exc:
            raise PlatformError("服务端返回了非 JSON 内容") from exc
        if not isinstance(data, dict):
            raise PlatformError("服务端返回了非对象 JSON")
        return data

    @staticmethod
    def loads_json(value: Any, default: Any) -> Any:
        if isinstance(value, (dict, list)):
            return value
        if not isinstance(value, str):
            return default
        try:
            return json.loads(value)
        except ValueError:
            return default

