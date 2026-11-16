"""Ordered, additive SQLite schema migrations."""

from __future__ import annotations

import logging
import shutil
import sqlite3
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

logger = logging.getLogger(__name__)

SCHEMA_VERSION = 1
BACKUP_KEEP = 5


@dataclass(frozen=True)
class Migration:
    version: int
    apply: Callable[[sqlite3.Connection], None]


def _add_account_activity_index(connection: sqlite3.Connection) -> None:
    connection.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_accounts_activity
            ON accounts(platform, status, last_refresh)
        """
    )


MIGRATIONS = (
    Migration(1, _add_account_activity_index),
)


def schema_version(connection: sqlite3.Connection) -> int:
    return int(connection.execute("PRAGMA user_version").fetchone()[0])


def _has_legacy_tables(connection: sqlite3.Connection) -> bool:
    row = connection.execute(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'accounts'"
    ).fetchone()
    return row is not None


def _make_backup(path: Path, source: sqlite3.Connection, version: int) -> Path:
    backup_dir = path.parent / "backups"
    backup_dir.mkdir(parents=True, exist_ok=True)
    timestamp = time.strftime("%Y%m%d-%H%M%S")
    destination = backup_dir / f"{path.stem}-v{version}-{timestamp}{path.suffix}"
    suffix = 1
    while destination.exists():
        destination = backup_dir / (
            f"{path.stem}-v{version}-{timestamp}-{suffix}{path.suffix}"
        )
        suffix += 1
    target = sqlite3.connect(destination)
    try:
        source.backup(target)
    finally:
        target.close()
    return destination


def _prune_backups(path: Path) -> None:
    backups = sorted(
        (path.parent / "backups").glob(f"{path.stem}-v*{path.suffix}"),
        key=lambda item: item.stat().st_mtime,
        reverse=True,
    )
    for item in backups[BACKUP_KEEP:]:
        try:
            item.unlink()
        except OSError:
            logger.warning("Could not remove old database backup: %s", item)


def migrate(
    connection: sqlite3.Connection,
    path: Path,
    *,
    had_legacy_data: bool | None = None,
) -> int:
    """Migrate a database transactionally, keeping a backup of legacy data."""
    current = schema_version(connection)
    if current > SCHEMA_VERSION:
        raise RuntimeError(
            f"Database schema v{current} is newer than supported v{SCHEMA_VERSION}"
        )
    pending = [item for item in MIGRATIONS if item.version > current]
    if not pending:
        return current

    backup = None
    if path.exists() and (
        had_legacy_data
        if had_legacy_data is not None
        else _has_legacy_tables(connection)
    ):
        backup = _make_backup(path, connection, current)
        logger.info("Database backup created at %s", backup)

    connection.execute("BEGIN IMMEDIATE")
    try:
        for migration in pending:
            migration.apply(connection)
            connection.execute(f"PRAGMA user_version = {migration.version}")
        connection.commit()
    except Exception:
        connection.rollback()
        if backup is not None:
            shutil.copy2(backup, path)
        raise
    else:
        if backup is not None:
            _prune_backups(path)
    return schema_version(connection)
