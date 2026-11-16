use crate::domain::{Account, CourseSnapshot, TaskSnapshot};
use anyhow::Context;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const SCHEMA_VERSION: i64 = 2;

const LEGACY_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
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

CREATE TABLE IF NOT EXISTS courses (
    account_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    course_id TEXT NOT NULL,
    name TEXT NOT NULL,
    progress INTEGER NOT NULL DEFAULT 0,
    raw TEXT NOT NULL DEFAULT '{}',
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(account_id, course_id)
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    course_id TEXT NOT NULL,
    course_name TEXT NOT NULL,
    title TEXT NOT NULL,
    remote_id TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    deadline TEXT,
    url TEXT NOT NULL DEFAULT '',
    detail TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'task-list',
    raw TEXT NOT NULL DEFAULT '{}',
    first_seen INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(account_id, platform, course_id, remote_id, title)
);

CREATE TABLE IF NOT EXISTS reminders (
    id TEXT PRIMARY KEY,
    account_id TEXT,
    task_id TEXT,
    title TEXT NOT NULL,
    remind_at TEXT NOT NULL,
    repeat_rule TEXT NOT NULL DEFAULT 'once',
    enabled INTEGER NOT NULL DEFAULT 1,
    last_triggered INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS completed_tasks (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    course_id TEXT NOT NULL,
    course_name TEXT NOT NULL,
    title TEXT NOT NULL,
    remote_id TEXT NOT NULL DEFAULT '',
    deadline TEXT,
    url TEXT NOT NULL DEFAULT '',
    detail TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT '',
    raw TEXT NOT NULL DEFAULT '{}',
    completed_at INTEGER NOT NULL,
    UNIQUE(task_id)
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS platform_checks (
    platform TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    checked_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tasks_account ON tasks(account_id);
CREATE INDEX IF NOT EXISTS idx_tasks_deadline ON tasks(deadline);
CREATE INDEX IF NOT EXISTS idx_reminders_time ON reminders(remind_at, enabled);
CREATE INDEX IF NOT EXISTS idx_completed_at ON completed_tasks(completed_at);
"#;

pub type Store = Arc<StoreInner>;

pub struct StoreInner {
    path: PathBuf,
    lock: Mutex<()>,
    secret_file: PathBuf,
}

fn has_application_tables(connection: &Connection) -> anyhow::Result<bool> {
    let found = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master
          WHERE type = 'table' AND name IN
            ('accounts', 'courses', 'tasks', 'reminders', 'completed_tasks',
             'settings', 'platform_checks'))",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(found != 0)
}

fn migrate(connection: &Connection, path: &Path, legacy: bool) -> anyhow::Result<()> {
    let current =
        connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?;
    anyhow::ensure!(
        current <= SCHEMA_VERSION,
        "Database schema v{current} is newer than supported v{SCHEMA_VERSION}"
    );
    if current == SCHEMA_VERSION {
        return Ok(());
    }

    let backup = if legacy {
        Some(backup_database(connection, path, current)?)
    } else {
        None
    };
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> anyhow::Result<()> {
        if legacy || current < 1 {
            connection.execute_batch(LEGACY_SCHEMA)?;
            connection.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_tasks_account ON tasks(account_id);
                 CREATE INDEX IF NOT EXISTS idx_tasks_deadline ON tasks(deadline);
                 CREATE INDEX IF NOT EXISTS idx_reminders_time ON reminders(remind_at, enabled);
                 CREATE INDEX IF NOT EXISTS idx_completed_at ON completed_tasks(completed_at);",
            )?;
        }
        if current < 2 {
            connection.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_accounts_activity
                    ON accounts(platform, status, last_refresh);",
            )?;
        }
        connection.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        connection.execute_batch("COMMIT")?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = connection.execute_batch("ROLLBACK");
        if let Some(backup) = backup.as_deref() {
            fs::copy(backup, path)?;
        }
        return Err(error);
    }
    if backup.is_some() {
        prune_backups(path);
    }
    Ok(())
}

fn backup_database(connection: &Connection, path: &Path, version: i64) -> anyhow::Result<PathBuf> {
    let directory = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("backups");
    fs::create_dir_all(&directory)?;
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("studypilot");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("db");
    let destination = directory.join(format!("{stem}-v{version}-{timestamp}.{extension}"));
    let mut destination = destination;
    let mut suffix = 1;
    while destination.exists() {
        destination = directory.join(format!(
            "{stem}-v{version}-{timestamp}-{suffix}.{extension}"
        ));
        suffix += 1;
    }
    let mut target = Connection::open(&destination)?;
    {
        let backup = rusqlite::backup::Backup::new(connection, &mut target)?;
        backup.run_to_completion(100, std::time::Duration::from_millis(10), None)?;
    }
    Ok(destination)
}

fn prune_backups(path: &Path) {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("studypilot");
    let directory = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("backups");
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut backups = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{stem}-v"))
        })
        .collect::<Vec<_>>();
    backups.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    backups.reverse();
    for entry in backups.into_iter().skip(5) {
        let _ = fs::remove_file(entry.path());
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn json_value(value: Option<String>, default: Value) -> Value {
    value
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(default)
}

fn optional_text_field(row: &Row<'_>, name: &str) -> rusqlite::Result<Option<String>> {
    row.get(name)
}

fn account_row(row: &Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: row.get("id")?,
        platform: row.get("platform")?,
        username: row.get("username")?,
        label: row.get("label")?,
        secret: None,
        display_name: row.get("display_name")?,
        user_id: row.get("user_id")?,
        school: row.get("school")?,
        session: json_value(row.get("session")?, json!({})),
        status: row.get("status")?,
        last_error: row.get("last_error")?,
        last_refresh: row.get("last_refresh")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn task_json(row: &Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, String>("id")?,
        "account_id": row.get::<_, String>("account_id")?,
        "platform": row.get::<_, String>("platform")?,
        "course_id": row.get::<_, String>("course_id")?,
        "course_name": row.get::<_, String>("course_name")?,
        "title": row.get::<_, String>("title")?,
        "task_id": row.get::<_, String>("remote_id")?,
        "status": row.get::<_, String>("status")?,
        "deadline": optional_text_field(row, "deadline")?,
        "url": row.get::<_, String>("url")?,
        "detail": row.get::<_, String>("detail")?,
        "source": row.get::<_, String>("source")?,
        "raw": json_value(row.get("raw")?, json!({})),
        "first_seen": row.get::<_, i64>("first_seen")?,
        "updated_at": row.get::<_, i64>("updated_at")?,
    }))
}

fn completed_task_json(row: &Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, String>("task_id")?,
        "completion_id": row.get::<_, String>("id")?,
        "account_id": row.get::<_, String>("account_id")?,
        "platform": row.get::<_, String>("platform")?,
        "course_id": row.get::<_, String>("course_id")?,
        "course_name": row.get::<_, String>("course_name")?,
        "title": row.get::<_, String>("title")?,
        "task_id": row.get::<_, String>("remote_id")?,
        "status": "completed",
        "deadline": row.get::<_, Option<String>>("deadline")?,
        "url": row.get::<_, String>("url")?,
        "detail": row.get::<_, String>("detail")?,
        "source": row.get::<_, String>("source")?,
        "raw": json_value(row.get("raw")?, json!({})),
        "completed_at": row.get::<_, i64>("completed_at")?,
    }))
}

fn json_text(value: &Value) -> String {
    if value.is_null() {
        "{}".to_owned()
    } else if let Some(value) = value.as_str() {
        value.to_owned()
    } else {
        value.to_string()
    }
}

fn course_text(course: &CourseSnapshot, name: &str, default: &str) -> String {
    let value = match name {
        "platform" => &course.platform,
        "id" => &course.id,
        "name" => &course.name,
        _ => return default.to_owned(),
    };
    if value.is_empty() {
        default.to_owned()
    } else {
        value.clone()
    }
}

fn task_text(task: &TaskSnapshot, name: &str, default: &str) -> String {
    let value = match name {
        "platform" => &task.platform,
        "course_id" => &task.course_id,
        "course_name" => &task.course_name,
        "title" => &task.title,
        "task_id" => &task.task_id,
        "status" => &task.status,
        "url" => &task.url,
        "detail" => &task.detail,
        "source" => &task.source,
        _ => return default.to_owned(),
    };
    if value.is_empty() {
        default.to_owned()
    } else {
        value.clone()
    }
}

#[cfg(test)]
pub fn create_test_task(
    store: &Store,
    account_id: &str,
    task_id: &str,
    title: &str,
) -> anyhow::Result<String> {
    store.upsert_task(
        account_id,
        &json!({
            "platform": "pigai",
            "course_id": "pigai:all",
            "course_name": "批改网作业",
            "title": title,
            "task_id": task_id,
            "status": "pending",
            "deadline": "2099-12-31 23:59",
            "url": "https://www.pigai.org/",
            "detail": "Rust API 回归测试",
            "source": "rust-test",
            "raw": {},
        }),
    )
}

impl StoreInner {
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Store> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let existed = path.exists();
        let secret_file = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("secret.key");
        let store = Arc::new(Self {
            secret_file,
            path: path.clone(),
            lock: Mutex::new(()),
        });
        let connection = store.connect()?;
        migrate(
            &connection,
            &path,
            existed || has_application_tables(&connection)?,
        )?;
        store.ensure_secret_key()?;
        Ok(store)
    }

    fn connect(&self) -> anyhow::Result<Connection> {
        let connection = Connection::open(&self.path)
            .with_context(|| format!("无法打开数据库 {}", self.path.display()))?;
        connection.busy_timeout(std::time::Duration::from_secs(20))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(connection)
    }

    #[cfg(test)]
    pub fn connect_for_test(&self) -> anyhow::Result<Connection> {
        self.connect()
    }

    pub fn database_path(&self) -> &Path {
        &self.path
    }

    fn ensure_secret_key(&self) -> anyhow::Result<()> {
        if self.secret_file.exists() {
            return Ok(());
        }
        self.write_secret_key()
    }

    fn write_secret_key(&self) -> anyhow::Result<()> {
        use std::io::Write;
        if let Some(parent) = self.secret_file.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut key = [0_u8; 32];
        getrandom::fill(&mut key).map_err(|error| anyhow::anyhow!("无法生成凭据密钥：{error}"))?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&self.secret_file) {
            Ok(mut file) => {
                file.write_all(&key)?;
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn list_accounts(&self) -> anyhow::Result<Vec<Account>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        let mut statement =
            connection.prepare("SELECT * FROM accounts ORDER BY created_at DESC")?;
        let rows = statement.query_map([], account_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_account(&self, account_id: &str) -> anyhow::Result<Option<Account>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        let account = connection
            .query_row(
                "SELECT * FROM accounts WHERE id = ?",
                [account_id],
                account_row,
            )
            .optional()?;
        Ok(account)
    }

    pub fn load_credentials(&self, account_id: &str) -> anyhow::Result<Option<(Account, String)>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        let row = connection
            .query_row("SELECT * FROM accounts WHERE id = ?", [account_id], |row| {
                Ok((account_row(row)?, row.get::<_, Option<Vec<u8>>>("secret")?))
            })
            .optional()?;
        let Some((account, protected)) = row else {
            return Ok(None);
        };
        let password = match protected {
            Some(value) => crate::security::unprotect_password(&value, &self.secret_file)?,
            None => String::new(),
        };
        Ok(Some((account, password)))
    }

    #[cfg(test)]
    pub fn secret_file(&self) -> PathBuf {
        self.secret_file.clone()
    }

    #[cfg(test)]
    pub fn credential_bytes(&self, account_id: &str) -> anyhow::Result<Vec<u8>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT COALESCE(secret, X'') FROM accounts WHERE id = ?",
                [account_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("账号不存在"))
    }

    #[cfg(test)]
    pub fn account_session(&self, account_id: &str) -> anyhow::Result<Option<Value>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        let session = connection
            .query_row(
                "SELECT session FROM accounts WHERE id = ?",
                [account_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(session
            .map(|value| json_value(Some(value), json!({})))
            .filter(Value::is_object))
    }

    pub fn session_bytes(&self, account_id: &str) -> anyhow::Result<Vec<u8>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        let session = connection
            .query_row(
                "SELECT session FROM accounts WHERE id = ?",
                [account_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("账号不存在"))?;
        Ok(session.into_bytes())
    }

    pub fn add_legacy_account(
        &self,
        platform: &str,
        username: &str,
        password: &str,
        label: &str,
    ) -> anyhow::Result<Account> {
        if username.trim().is_empty() {
            anyhow::bail!("账号不能为空");
        }
        if password.is_empty() {
            anyhow::bail!("密码不能为空");
        }
        let protected = self.protect_password(password)?;
        let _guard = self.lock.lock().unwrap();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let existing: Option<String> = transaction
            .query_row(
                "SELECT id FROM accounts WHERE platform = ? AND username = ?",
                [platform, username],
                |row| row.get(0),
            )
            .optional()?;
        let timestamp = now();
        let id = existing
            .as_ref()
            .cloned()
            .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
        if existing.is_some() {
            transaction.execute(
                "UPDATE accounts SET label = ?, secret = ?, status = 'ready',
                 last_error = '', updated_at = ? WHERE id = ?",
                params![label, protected, timestamp, id],
            )?;
        } else {
            transaction.execute(
                "INSERT INTO accounts
                 (id, platform, username, label, secret, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    id, platform, username, label, protected, timestamp, timestamp
                ],
            )?;
        }
        transaction.commit()?;
        drop(_guard);
        self.get_account(&id)?
            .ok_or_else(|| anyhow::anyhow!("账号创建后未能读取"))
    }

    pub fn delete_account(&self, account_id: &str) -> anyhow::Result<bool> {
        let _guard = self.lock.lock().unwrap();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM tasks WHERE account_id = ?", [account_id])?;
        transaction.execute("DELETE FROM courses WHERE account_id = ?", [account_id])?;
        transaction.execute("DELETE FROM reminders WHERE account_id = ?", [account_id])?;
        let affected = transaction.execute("DELETE FROM accounts WHERE id = ?", [account_id])?;
        transaction.commit()?;
        Ok(affected > 0)
    }

    pub fn replace_account_data(
        &self,
        account_id: &str,
        courses: &[CourseSnapshot],
        tasks: &[TaskSnapshot],
    ) -> anyhow::Result<()> {
        let timestamp = now();
        let _guard = self.lock.lock().unwrap();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM courses WHERE account_id = ?", [account_id])?;
        for course in courses {
            transaction.execute(
                "INSERT INTO courses
                 (account_id, platform, course_id, name, progress, raw, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    account_id,
                    course_text(course, "platform", ""),
                    course_text(course, "id", ""),
                    course_text(course, "name", ""),
                    course.progress,
                    json_text(&course.raw),
                    timestamp,
                ],
            )?;
        }
        let previous: HashMap<String, (String, String)> = transaction
            .prepare(
                "SELECT id, status, account_id || '|' || platform || '|' || course_id || '|' ||
                        remote_id || '|' || title AS task_key
                 FROM tasks WHERE account_id = ?",
            )?
            .query_map([account_id], |row| {
                Ok((row.get(2)?, (row.get(0)?, row.get(1)?)))
            })?
            .collect::<Result<_, _>>()?;
        let mut current_keys = HashSet::new();
        for task in tasks {
            let platform = task_text(task, "platform", "");
            let course_id = task_text(task, "course_id", "");
            let remote_id = task_text(task, "task_id", "");
            let title = task_text(task, "title", "");
            let key = format!("{account_id}|{platform}|{course_id}|{remote_id}|{title}");
            current_keys.insert(key.clone());
            let task_id = previous
                .get(&key)
                .map(|(id, _)| id.clone())
                .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
            transaction.execute(
                "INSERT INTO tasks
                 (id, account_id, platform, course_id, course_name, title, remote_id,
                  status, deadline, url, detail, source, raw, first_seen, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, platform, course_id, remote_id, title)
                 DO UPDATE SET course_name = excluded.course_name,
                               status = CASE
                                   WHEN tasks.status IN ('completed', 'ignored')
                                   THEN tasks.status
                                   ELSE excluded.status
                               END,
                               deadline = COALESCE(tasks.deadline, excluded.deadline),
                               url = excluded.url, detail = excluded.detail,
                               source = excluded.source, raw = excluded.raw,
                               updated_at = excluded.updated_at",
                params![
                    task_id,
                    account_id,
                    platform,
                    course_id,
                    task_text(task, "course_name", ""),
                    title,
                    remote_id,
                    task_text(task, "status", "pending"),
                    task.deadline,
                    task_text(task, "url", ""),
                    task_text(task, "detail", ""),
                    task_text(task, "source", "task-list"),
                    json_text(&task.raw),
                    timestamp,
                    timestamp,
                ],
            )?;
        }
        for (key, (task_id, status)) in previous {
            if !current_keys.contains(&key) {
                if status == "completed" || status == "ignored" {
                    continue;
                }
                transaction.execute("DELETE FROM tasks WHERE id = ?", [task_id])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn mark_refreshing(&self, account_id: &str) -> anyhow::Result<bool> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        Ok(connection.execute(
            "UPDATE accounts
             SET status = 'refreshing', last_error = '', updated_at = ?
             WHERE id = ?",
            params![now(), account_id],
        )? > 0)
    }

    pub fn update_account(
        &self,
        account_id: &str,
        status: &str,
        last_error: &str,
        session: &Value,
        display_name: Option<&str>,
        user_id: Option<&str>,
        school: Option<&str>,
        last_refresh: i64,
    ) -> anyhow::Result<bool> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        Ok(connection.execute(
            "UPDATE accounts
             SET status = ?, last_error = ?, session = ?,
                 display_name = COALESCE(?, display_name),
                 user_id = COALESCE(?, user_id),
                 school = COALESCE(?, school),
                 last_refresh = CASE WHEN ? > 0 THEN ? ELSE last_refresh END,
                 updated_at = ?
             WHERE id = ?",
            params![
                status,
                last_error,
                session.to_string(),
                display_name,
                user_id,
                school,
                last_refresh,
                last_refresh,
                now(),
                account_id
            ],
        )? > 0)
    }

    pub fn replace_account_snapshot(
        &self,
        account_id: &str,
        session: &Value,
        display_name: Option<&str>,
        user_id: Option<&str>,
        school: Option<&str>,
        last_refresh: i64,
        courses: &[CourseSnapshot],
        tasks: &[TaskSnapshot],
    ) -> anyhow::Result<bool> {
        let _guard = self.lock.lock().unwrap();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let changed = transaction.execute(
            "UPDATE accounts
             SET status = 'ready', last_error = '', session = ?,
                 display_name = COALESCE(?, display_name),
                 user_id = COALESCE(?, user_id),
                 school = COALESCE(?, school),
                 last_refresh = CASE WHEN ? > 0 THEN ? ELSE last_refresh END,
                 updated_at = ?
             WHERE id = ?",
            params![
                session.to_string(),
                display_name,
                user_id,
                school,
                last_refresh,
                last_refresh,
                now(),
                account_id
            ],
        )?;
        if changed == 0 {
            transaction.rollback()?;
            return Ok(false);
        }
        transaction.execute("DELETE FROM courses WHERE account_id = ?", [account_id])?;
        let timestamp = now();
        for course in courses {
            transaction.execute(
                "INSERT INTO courses
                 (account_id, platform, course_id, name, progress, raw, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    account_id,
                    course_text(course, "platform", ""),
                    course_text(course, "id", ""),
                    course_text(course, "name", ""),
                    course.progress,
                    json_text(&course.raw),
                    timestamp,
                ],
            )?;
        }
        let previous: HashMap<String, (String, String)> = transaction
            .prepare(
                "SELECT id, status, account_id || '|' || platform || '|' || course_id || '|' ||
                        remote_id || '|' || title AS task_key
                 FROM tasks WHERE account_id = ?",
            )?
            .query_map([account_id], |row| {
                Ok((row.get(2)?, (row.get(0)?, row.get(1)?)))
            })?
            .collect::<Result<_, _>>()?;
        let mut current_keys = HashSet::new();
        for task in tasks {
            let platform = task_text(task, "platform", "");
            let course_id = task_text(task, "course_id", "");
            let remote_id = task_text(task, "task_id", "");
            let title = task_text(task, "title", "");
            let key = format!("{account_id}|{platform}|{course_id}|{remote_id}|{title}");
            current_keys.insert(key.clone());
            let task_id = previous
                .get(&key)
                .map(|(id, _)| id.clone())
                .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
            transaction.execute(
                "INSERT INTO tasks
                 (id, account_id, platform, course_id, course_name, title, remote_id,
                  status, deadline, url, detail, source, raw, first_seen, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, platform, course_id, remote_id, title)
                 DO UPDATE SET course_name = excluded.course_name,
                               status = CASE
                                   WHEN tasks.status IN ('completed', 'ignored')
                                   THEN tasks.status
                                   ELSE excluded.status
                               END,
                               deadline = COALESCE(tasks.deadline, excluded.deadline),
                               url = excluded.url, detail = excluded.detail,
                               source = excluded.source, raw = excluded.raw,
                               updated_at = excluded.updated_at",
                params![
                    task_id,
                    account_id,
                    platform,
                    course_id,
                    task_text(task, "course_name", ""),
                    title,
                    remote_id,
                    task_text(task, "status", "pending"),
                    task.deadline,
                    task_text(task, "url", ""),
                    task_text(task, "detail", ""),
                    task_text(task, "source", "task-list"),
                    json_text(&task.raw),
                    timestamp,
                    timestamp,
                ],
            )?;
        }
        for (key, (task_id, status)) in previous {
            if !current_keys.contains(&key) {
                if status == "completed" || status == "ignored" {
                    continue;
                }
                transaction.execute("DELETE FROM tasks WHERE id = ?", [task_id])?;
            }
        }
        transaction.commit()?;
        Ok(true)
    }

    pub fn note_refresh_error(&self, account_id: &str, error: &str) -> anyhow::Result<bool> {
        let status = if error.contains("登录态已失效") || error.contains("登录失败") {
            "auth_error"
        } else {
            "error"
        };
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        Ok(connection.execute(
            "UPDATE accounts
             SET status = ?, last_error = ?, updated_at = ?
             WHERE id = ?",
            params![status, error, now(), account_id],
        )? > 0)
    }

    pub fn list_tasks(
        &self,
        account_id: Option<&str>,
        status: Option<&str>,
        platform: Option<&str>,
    ) -> anyhow::Result<Vec<Value>> {
        let mut query = "SELECT * FROM tasks WHERE 1=1".to_owned();
        let mut params = Vec::<rusqlite::types::Value>::new();
        for (column, value) in [
            ("account_id", account_id),
            ("status", status),
            ("platform", platform),
        ] {
            if let Some(value) = value {
                query.push_str(&format!(" AND {column} = ?"));
                params.push(rusqlite::types::Value::Text(value.to_owned()));
            }
        }
        query.push_str(
            " ORDER BY CASE WHEN deadline IS NULL OR deadline = '' THEN 1 ELSE 0 END,
              deadline ASC, updated_at DESC",
        );
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        let mut statement = connection.prepare(&query)?;
        let rows = statement.query_map(rusqlite::params_from_iter(params), task_json)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_task(&self, task_id: &str) -> anyhow::Result<Option<Value>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        Ok(connection
            .query_row("SELECT * FROM tasks WHERE id = ?", [task_id], task_json)
            .optional()?)
    }

    pub fn upsert_task(&self, account_id: &str, task: &Value) -> anyhow::Result<String> {
        let snapshot: TaskSnapshot = serde_json::from_value(task.clone())?;
        self.upsert_task_snapshot(account_id, &snapshot)
    }

    pub fn upsert_task_snapshot(
        &self,
        account_id: &str,
        task: &TaskSnapshot,
    ) -> anyhow::Result<String> {
        let timestamp = now();
        let platform = task_text(task, "platform", "");
        let course_id = task_text(task, "course_id", "");
        let remote_id = task_text(task, "task_id", "");
        let title = task_text(task, "title", "");
        let _guard = self.lock.lock().unwrap();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let existing: Option<String> = transaction
            .query_row(
                "SELECT id FROM tasks WHERE account_id = ? AND platform = ? AND
                 course_id = ? AND remote_id = ? AND title = ?",
                params![account_id, platform, course_id, remote_id, title],
                |row| row.get(0),
            )
            .optional()?;
        let task_id = existing.unwrap_or_else(|| Uuid::new_v4().simple().to_string());
        transaction.execute(
            "INSERT INTO tasks
             (id, account_id, platform, course_id, course_name, title, remote_id,
              status, deadline, url, detail, source, raw, first_seen, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id, platform, course_id, remote_id, title)
             DO UPDATE SET course_name = excluded.course_name,
                           status = CASE
                               WHEN tasks.status IN ('completed', 'ignored')
                               THEN tasks.status
                               ELSE excluded.status
                           END,
                           deadline = COALESCE(tasks.deadline, excluded.deadline),
                           url = excluded.url, detail = excluded.detail,
                           source = excluded.source, raw = excluded.raw,
                           updated_at = excluded.updated_at",
            params![
                task_id,
                account_id,
                platform,
                course_id,
                task_text(task, "course_name", ""),
                title,
                remote_id,
                task_text(task, "status", "pending"),
                task.deadline,
                task_text(task, "url", ""),
                task_text(task, "detail", ""),
                task_text(task, "source", "essay-lookup"),
                json_text(&task.raw),
                timestamp,
                timestamp,
            ],
        )?;
        transaction.commit()?;
        Ok(task_id)
    }

    pub fn update_task_status(&self, task_id: &str, status: &str) -> anyhow::Result<bool> {
        let timestamp = now();
        let _guard = self.lock.lock().unwrap();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let mut statement = transaction.prepare("SELECT * FROM tasks WHERE id = ?")?;
        let task = statement.query_row([task_id], task_json).optional()?;
        drop(statement);
        let Some(task) = task else {
            return Ok(false);
        };
        if status == "completed" {
            transaction.execute(
                "INSERT INTO completed_tasks
                 (id, task_id, account_id, platform, course_id, course_name, title,
                  remote_id, deadline, url, detail, source, raw, completed_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(task_id) DO UPDATE SET course_name = excluded.course_name,
                    title = excluded.title, deadline = completed_tasks.deadline,
                    url = excluded.url, detail = excluded.detail, raw = excluded.raw,
                    completed_at = excluded.completed_at",
                params![
                    Uuid::new_v4().simple().to_string(),
                    task_id,
                    task["account_id"].as_str().unwrap_or(""),
                    task["platform"].as_str().unwrap_or(""),
                    task["course_id"].as_str().unwrap_or(""),
                    task["course_name"].as_str().unwrap_or(""),
                    task["title"].as_str().unwrap_or(""),
                    task["task_id"].as_str().unwrap_or(""),
                    task["deadline"].as_str(),
                    task["url"].as_str().unwrap_or(""),
                    task["detail"].as_str().unwrap_or(""),
                    task["source"].as_str().unwrap_or(""),
                    task["raw"].to_string(),
                    timestamp,
                ],
            )?;
        } else {
            transaction.execute("DELETE FROM completed_tasks WHERE task_id = ?", [task_id])?;
        }
        let affected = transaction.execute(
            "UPDATE tasks SET status = ?, updated_at = ? WHERE id = ?",
            params![status, timestamp, task_id],
        )?;
        purge_completed(&transaction)?;
        transaction.commit()?;
        Ok(affected > 0)
    }

    pub fn list_completed_tasks(&self, days: i64) -> anyhow::Result<Vec<Value>> {
        let cutoff = now() - days.clamp(1, 365) * 24 * 60 * 60;
        let _guard = self.lock.lock().unwrap();
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        purge_completed(&transaction)?;
        let mut statement = transaction.prepare(
            "SELECT * FROM completed_tasks WHERE completed_at >= ? ORDER BY completed_at DESC",
        )?;
        let tasks = statement
            .query_map([cutoff], completed_task_json)?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        transaction.commit()?;
        Ok(tasks)
    }

    pub fn list_courses(&self, account_id: Option<&str>) -> anyhow::Result<Vec<Value>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        let (query, params) = match account_id {
            Some(id) => (
                "SELECT * FROM courses WHERE account_id = ? ORDER BY progress ASC, name ASC",
                vec![rusqlite::types::Value::Text(id.to_owned())],
            ),
            None => (
                "SELECT * FROM courses ORDER BY progress ASC, name ASC",
                Vec::new(),
            ),
        };
        let mut statement = connection.prepare(query)?;
        let rows = statement.query_map(rusqlite::params_from_iter(params), |row| {
            Ok(json!({
                "account_id": row.get::<_, String>("account_id")?,
                "platform": row.get::<_, String>("platform")?,
                "id": row.get::<_, String>("course_id")?,
                "name": row.get::<_, String>("name")?,
                "progress": row.get::<_, i64>("progress")?,
                "raw": json_value(row.get("raw")?, json!({})),
                "updated_at": row.get::<_, i64>("updated_at")?,
            }))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn add_reminder(
        &self,
        title: &str,
        remind_at: &str,
        account_id: Option<&str>,
        task_id: Option<&str>,
        repeat_rule: &str,
    ) -> anyhow::Result<Value> {
        let reminder_id = Uuid::new_v4().simple().to_string();
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO reminders
             (id, account_id, task_id, title, remind_at, repeat_rule, enabled, created_at)
             VALUES (?, ?, ?, ?, ?, ?, 1, ?)",
            params![
                reminder_id,
                account_id,
                task_id,
                title,
                remind_at,
                repeat_rule,
                now()
            ],
        )?;
        drop(_guard);
        self.get_reminder(&reminder_id)?
            .ok_or_else(|| anyhow::anyhow!("提醒创建后未能读取"))
    }

    pub fn get_reminder(&self, reminder_id: &str) -> anyhow::Result<Option<Value>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        Ok(connection
            .query_row(
                "SELECT * FROM reminders WHERE id = ?",
                [reminder_id],
                reminder_json,
            )
            .optional()?)
    }

    pub fn list_reminders(&self) -> anyhow::Result<Vec<Value>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT r.*, t.title AS task_title, a.platform, a.label AS account_label
             FROM reminders r
             LEFT JOIN tasks t ON t.id = r.task_id
             LEFT JOIN accounts a ON a.id = r.account_id
             ORDER BY r.enabled DESC, r.remind_at ASC",
        )?;
        let rows = statement.query_map([], |row| {
            let mut value = reminder_json(row)?;
            value["task_title"] = row
                .get::<_, Option<String>>("task_title")?
                .map_or(Value::Null, Value::String);
            value["platform"] = row
                .get::<_, Option<String>>("platform")?
                .map_or(Value::Null, Value::String);
            value["account_label"] = row
                .get::<_, Option<String>>("account_label")?
                .map_or(Value::Null, Value::String);
            Ok(value)
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn update_reminder(&self, reminder_id: &str, fields: &Value) -> anyhow::Result<bool> {
        let mut columns = Vec::new();
        let mut values = Vec::new();
        for key in ["title", "remind_at", "repeat_rule"] {
            if let Some(value) = fields.get(key).and_then(Value::as_str) {
                columns.push(format!("{key} = ?"));
                values.push(rusqlite::types::Value::Text(value.to_owned()));
            }
        }
        if let Some(value) = fields.get("enabled").and_then(Value::as_bool) {
            columns.push("enabled = ?".to_owned());
            values.push(rusqlite::types::Value::Integer(i64::from(value)));
        }
        if columns.is_empty() {
            return Ok(false);
        }
        values.push(rusqlite::types::Value::Text(reminder_id.to_owned()));
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        Ok(connection.execute(
            &format!("UPDATE reminders SET {} WHERE id = ?", columns.join(", ")),
            rusqlite::params_from_iter(values),
        )? > 0)
    }

    pub fn delete_reminder(&self, reminder_id: &str) -> anyhow::Result<bool> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        Ok(connection.execute("DELETE FROM reminders WHERE id = ?", [reminder_id])? > 0)
    }

    pub fn claim_due_reminder(
        &self,
        reminder_id: &str,
        now: chrono::NaiveDateTime,
    ) -> anyhow::Result<Option<Value>> {
        let _guard = self.lock.lock().unwrap();
        let mut connection = self.connect()?;
        crate::reminders::claim_due_reminder(&mut connection, reminder_id, now)
    }

    pub fn set_platform_check(
        &self,
        platform: &str,
        status: &str,
        note: &str,
    ) -> anyhow::Result<()> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        connection.execute(
            "INSERT INTO platform_checks(platform, status, note, checked_at)
             VALUES(?, ?, ?, ?)
             ON CONFLICT(platform) DO UPDATE SET status = excluded.status,
                note = excluded.note, checked_at = excluded.checked_at",
            params![platform, status, note, now()],
        )?;
        Ok(())
    }

    pub fn get_platform_checks(&self) -> anyhow::Result<serde_json::Map<String, Value>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        let mut statement =
            connection.prepare("SELECT * FROM platform_checks ORDER BY platform")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>("platform")?,
                json!({
                    "status": row.get::<_, String>("status")?,
                    "note": row.get::<_, String>("note")?,
                    "checked_at": row.get::<_, i64>("checked_at")?,
                }),
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    #[cfg(test)]
    pub fn get_setting(&self, key: &str) -> anyhow::Result<Option<Value>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        let value = connection
            .query_row("SELECT value FROM settings WHERE key = ?", [key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        Ok(value.map(|value| json_value(Some(value), Value::Null)))
    }

    fn protect_password(&self, password: &str) -> anyhow::Result<Vec<u8>> {
        crate::security::protect_password(password, &self.secret_file)
    }
}

fn reminder_json(row: &Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, String>("id")?,
        "account_id": row.get::<_, Option<String>>("account_id")?,
        "task_id": row.get::<_, Option<String>>("task_id")?,
        "title": row.get::<_, String>("title")?,
        "remind_at": row.get::<_, String>("remind_at")?,
        "repeat_rule": row.get::<_, String>("repeat_rule")?,
        "enabled": row.get::<_, i64>("enabled")? != 0,
        "last_triggered": row.get::<_, i64>("last_triggered")?,
        "created_at": row.get::<_, i64>("created_at")?,
    }))
}

fn purge_completed(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute(
        "DELETE FROM completed_tasks WHERE completed_at < ?",
        [now() - 7 * 24 * 60 * 60],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_task(title: &str) -> TaskSnapshot {
        TaskSnapshot {
            platform: "pigai".to_owned(),
            course_id: "pigai:all".to_owned(),
            course_name: "批改网作业".to_owned(),
            title: title.to_owned(),
            task_id: "1".to_owned(),
            status: "pending".to_owned(),
            deadline: Some("2026-11-11 23:59".to_owned()),
            url: "https://www.pigai.org/".to_owned(),
            detail: "作文".to_owned(),
            source: "wait-complete".to_owned(),
            raw: json!({}),
        }
    }

    #[test]
    fn account_and_task_cache_lifecycle() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let account = store
            .add_legacy_account("pigai", "tester", "test-password", "测试账号")
            .unwrap();
        let id = account.id.as_str();
        assert_eq!(account.username, "tester");
        store
            .replace_account_data(id, &[], &[sample_task("第1号 作文")])
            .unwrap();
        assert_eq!(store.list_tasks(None, None, None).unwrap().len(), 1);
        store.replace_account_data(id, &[], &[]).unwrap();
        assert!(store.list_tasks(None, None, None).unwrap().is_empty());
        assert!(store.delete_account(id).unwrap());
    }

    #[test]
    fn completed_tasks_can_be_restored() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let account = store
            .add_legacy_account("pigai", "tester", "test-password", "")
            .unwrap();
        let id = account.id.as_str();
        let task_id = store
            .upsert_task_snapshot(id, &sample_task("第1号 作文"))
            .unwrap();
        assert!(store.update_task_status(&task_id, "completed").unwrap());
        let completed = store.list_completed_tasks(7).unwrap();
        assert_eq!(completed[0]["title"], "第1号 作文");
        assert!(store.update_task_status(&task_id, "pending").unwrap());
        assert!(store.list_completed_tasks(7).unwrap().is_empty());
    }

    #[test]
    fn refresh_keeps_manual_task_status() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let account = store
            .add_legacy_account("pigai", "tester", "test-password", "")
            .unwrap();
        let id = account.id.as_str();
        let task = sample_task("第1号 作文");
        let task_id = store.upsert_task_snapshot(id, &task).unwrap();
        assert!(store.update_task_status(&task_id, "completed").unwrap());

        store
            .replace_account_data(id, &[], std::slice::from_ref(&task))
            .unwrap();

        let tasks = store.list_tasks(Some(id), None, None).unwrap();
        assert_eq!(tasks[0]["status"], "completed");
        assert_eq!(store.list_completed_tasks(7).unwrap().len(), 1);
    }

    #[test]
    fn refresh_keeps_manual_tasks_removed_upstream() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let account = store
            .add_legacy_account("pigai", "tester", "test-password", "")
            .unwrap();
        let id = account.id.as_str();
        let task_id = store
            .upsert_task_snapshot(id, &sample_task("第1号 作文"))
            .unwrap();
        assert!(store.update_task_status(&task_id, "completed").unwrap());

        store.replace_account_data(id, &[], &[]).unwrap();

        let completed = store.list_completed_tasks(7).unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0]["id"], task_id);
    }

    #[test]
    fn refresh_keeps_existing_deadline_when_upstream_has_none() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let account = store
            .add_legacy_account("pigai", "tester", "test-password", "")
            .unwrap();
        let id = account.id.as_str();
        let task = sample_task("第1号 作文");
        store
            .upsert_task_snapshot(id, &task)
            .expect("initial task insert");
        store
            .upsert_task_snapshot(
                id,
                &TaskSnapshot {
                    deadline: None,
                    ..task
                },
            )
            .expect("refresh task update");

        assert_eq!(
            store.list_tasks(Some(id), None, None).unwrap()[0]["deadline"],
            "2026-11-11 23:59"
        );
    }

    #[test]
    fn reminder_lifecycle() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let reminder = store
            .add_reminder("完成作文", "2026-11-11T20:00", None, None, "once")
            .unwrap();
        let id = reminder["id"].as_str().unwrap();
        assert!(
            store
                .update_reminder(id, &json!({"enabled": false}))
                .unwrap()
        );
        assert_eq!(store.get_reminder(id).unwrap().unwrap()["enabled"], false);
        assert!(store.delete_reminder(id).unwrap());
    }

    #[test]
    fn secret_file_is_created_once() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let first = fs::read(store.secret_file.clone()).unwrap();
        store.ensure_secret_key().unwrap();
        let second = fs::read(store.secret_file.clone()).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 32);
    }

    #[test]
    fn legacy_database_is_backed_up_and_keeps_sessions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("legacy.db");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch(LEGACY_SCHEMA).unwrap();
        connection
            .execute(
                "INSERT INTO accounts
                 (id, platform, username, label, session, display_name, status,
                  last_refresh, created_at, updated_at)
                 VALUES ('legacy-account', 'pigai', 'student', 'Legacy',
                         '{\"cookies\": [{\"name\": \"auth\"}]}', 'Student',
                         'ready', 42, 1, 1)",
                [],
            )
            .unwrap();
        drop(connection);

        let store = StoreInner::open(&path).unwrap();
        let account = store.get_account("legacy-account").unwrap().unwrap();
        assert_eq!(account.display_name, "Student");
        assert_eq!(account.session["cookies"][0]["name"], "auth");
        let connection = store.connect().unwrap();
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );
        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index'")
            .unwrap();
        let indexes = statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(indexes.iter().any(|name| name == "idx_accounts_activity"));
        assert_eq!(
            directory.path().join("backups").read_dir().unwrap().count(),
            1
        );
        assert!(
            store
                .add_legacy_account("pigai", "student", "new-secret", "")
                .is_ok()
        );
    }

    #[test]
    fn copied_legacy_database_keeps_credentials_and_cached_workflows() {
        let directory = tempfile::tempdir().unwrap();
        let source_dir = directory.path().join("source");
        fs::create_dir_all(&source_dir).unwrap();
        let source = StoreInner::open(source_dir.join("studypilot.db")).unwrap();
        let account = source
            .add_legacy_account("pigai", "legacy-student", "legacy-secret", "旧账号")
            .unwrap();
        let task = TaskSnapshot {
            status: "completed".to_owned(),
            ..sample_task("旧任务")
        };
        let task_id = source.upsert_task_snapshot(&account.id, &task).unwrap();
        source
            .add_reminder(
                "旧提醒",
                "2099-12-31T20:00",
                Some(&account.id),
                Some(&task_id),
                "weekly",
            )
            .unwrap();
        source
            .set_platform_check("pigai", "verified", "旧数据库验证")
            .unwrap();
        source
            .connect()
            .unwrap()
            .execute(
                "INSERT INTO settings(key, value) VALUES ('legacy-key', ?)",
                [r#"{"enabled":true}"#],
            )
            .unwrap();

        let copied_dir = directory.path().join("copied");
        fs::create_dir_all(&copied_dir).unwrap();
        fs::copy(
            source_dir.join("studypilot.db"),
            copied_dir.join("studypilot.db"),
        )
        .unwrap();
        fs::copy(source_dir.join("secret.key"), copied_dir.join("secret.key")).unwrap();
        assert!(
            fs::read(copied_dir.join("secret.key"))
                .unwrap()
                .iter()
                .any(|byte| *byte != 0)
        );

        let copied = StoreInner::open(copied_dir.join("studypilot.db")).unwrap();
        let copied_account = copied.get_account(&account.id).unwrap().unwrap();
        assert_eq!(copied_account.session["version"], Value::Null);
        assert_eq!(copied_account.last_refresh, 0);
        let (_, password) = copied.load_credentials(&account.id).unwrap().unwrap();
        assert_eq!(password, "legacy-secret");
        let copied_tasks = copied.list_tasks(Some(&account.id), None, None).unwrap();
        assert_eq!(copied_tasks.len(), 1);
        assert_eq!(copied_tasks[0]["status"], "completed");
        let reminders = copied.list_reminders().unwrap();
        assert_eq!(reminders.len(), 1);
        assert_eq!(reminders[0]["repeat_rule"], "weekly");
        assert_eq!(
            copied.get_setting("legacy-key").unwrap().unwrap()["enabled"],
            true
        );
        assert_eq!(
            copied.get_platform_checks().unwrap()["pigai"]["status"],
            "verified"
        );
    }
}
