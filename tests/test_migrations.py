import sqlite3

from studypilot.storage import Store
from studypilot.storage_migrations import SCHEMA_VERSION, migrate


LEGACY_SCHEMA = """
CREATE TABLE accounts (
    id TEXT PRIMARY KEY,
    platform TEXT NOT NULL,
    username TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    secret BLOB,
    display_name TEXT NOT NULL DEFAULT '',
    user_id TEXT NOT NULL DEFAULT '',
    school TEXT NOT NULL DEFAULT '',
    session TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'ready',
    last_error TEXT NOT NULL DEFAULT '',
    last_refresh INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(platform, username)
);
"""


def test_legacy_database_is_backed_up_before_upgrade(tmp_path):
    path = tmp_path / "legacy.db"
    connection = sqlite3.connect(path)
    connection.executescript(LEGACY_SCHEMA)
    connection.execute(
        """
        INSERT INTO accounts
            (id, platform, username, display_name, session, last_refresh,
             created_at, updated_at)
        VALUES ('account-1', 'pigai', 'legacy', 'Legacy User',
                '{}', 42, 10, 10)
        """
    )
    connection.commit()
    assert connection.execute("PRAGMA user_version").fetchone()[0] == 0
    assert migrate(connection, path) == SCHEMA_VERSION
    connection.close()

    backups = list((tmp_path / "backups").glob("legacy-v0-*.db"))
    assert len(backups) == 1
    backup = sqlite3.connect(backups[0])
    try:
        assert backup.execute("PRAGMA user_version").fetchone()[0] == 0
        assert backup.execute(
            "SELECT username FROM accounts WHERE id = 'account-1'"
        ).fetchone() == ("legacy",)
    finally:
        backup.close()


def test_store_migration_is_idempotent_and_preserves_activity(tmp_path):
    store = Store(tmp_path / "studypilot.db")
    account = store.add_account("pigai", "first", "secret")
    assert store.update_account(account["id"], last_refresh=123)
    store = Store(tmp_path / "studypilot.db")
    loaded = store.get_account(account["id"])
    assert loaded["last_refresh"] == 123
    connection = sqlite3.connect(tmp_path / "studypilot.db")
    try:
        assert connection.execute("PRAGMA user_version").fetchone()[0] == 1
    finally:
        connection.close()
