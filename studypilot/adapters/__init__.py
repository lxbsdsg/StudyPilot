"""Platform adapter registry."""

from .base import PlatformAdapter, PlatformAuthError, PlatformError
from .chaoxing import ChaoxingAdapter
from .pigai import PigaiAdapter
from .unipus import UnipusAdapter
from .welearn import WelearnAdapter

ADAPTERS: dict[str, type[PlatformAdapter]] = {
    "unipus": UnipusAdapter,
    "welearn": WelearnAdapter,
    "chaoxing": ChaoxingAdapter,
    "pigai": PigaiAdapter,
}


def create_adapter(platform: str, account: dict) -> PlatformAdapter:
    try:
        adapter_cls = ADAPTERS[platform]
    except KeyError as exc:
        raise PlatformError(f"不支持的平台：{platform}") from exc
    return adapter_cls(account)


__all__ = [
    "ADAPTERS",
    "ChaoxingAdapter",
    "PigaiAdapter",
    "PlatformAdapter",
    "PlatformAuthError",
    "PlatformError",
    "UnipusAdapter",
    "WelearnAdapter",
    "create_adapter",
]

