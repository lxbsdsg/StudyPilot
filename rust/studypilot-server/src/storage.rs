use anyhow::Context;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Value, json};
#[cfg(test)]
use std::collections::{HashMap, HashSet};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const SCHEMA: &str = r#"
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

fn account_json(row: &Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, String>("id")?,
        "platform": row.get::<_, String>("platform")?,
        "username": row.get::<_, String>("username")?,
        "label": row.get::<_, String>("label")?,
        "display_name": row.get::<_, String>("display_name")?,
        "user_id": row.get::<_, String>("user_id")?,
        "school": row.get::<_, String>("school")?,
        "session": json_value(row.get("session")?, json!({})),
        "status": row.get::<_, String>("status")?,
        "last_error": row.get::<_, String>("last_error")?,
        "last_refresh": row.get::<_, i64>("last_refresh")?,
        "created_at": row.get::<_, i64>("created_at")?,
        "updated_at": row.get::<_, i64>("updated_at")?,
    }))
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

#[cfg(test)]
fn string_field(value: &Value, name: &str, default: &str) -> String {
    value
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_owned()
}

pub fn redact_secret_in_place(value: &mut Value) {
    if let Value::Object(object) = value {
        object.remove("password");
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
        let store = Arc::new(Self {
            secret_file: path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("secret.key"),
            path,
            lock: Mutex::new(()),
        });
        let connection = store.connect()?;
        connection.execute_batch(SCHEMA)?;
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

    pub fn list_accounts(&self) -> anyhow::Result<Vec<Value>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        let mut statement =
            connection.prepare("SELECT * FROM accounts ORDER BY created_at DESC")?;
        let rows = statement.query_map([], account_json)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_account(&self, account_id: &str) -> anyhow::Result<Option<Value>> {
        let _guard = self.lock.lock().unwrap();
        let connection = self.connect()?;
        let mut account = connection
            .query_row(
                "SELECT * FROM accounts WHERE id = ?",
                [account_id],
                account_json,
            )
            .optional()?;
        if let Some(account) = account.as_mut() {
            redact_secret_in_place(account);
        }
        Ok(account)
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

    pub fn add_legacy_account(
        &self,
        platform: &str,
        username: &str,
        password: &str,
        label: &str,
    ) -> anyhow::Result<Value> {
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

    #[cfg(test)]
    pub fn replace_account_data(
        &self,
        account_id: &str,
        courses: &[Value],
        tasks: &[Value],
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
                    string_field(course, "platform", ""),
                    string_field(course, "id", ""),
                    string_field(course, "name", ""),
                    course.get("progress").and_then(Value::as_i64).unwrap_or(0),
                    string_field(course, "raw", "{}"),
                    timestamp,
                ],
            )?;
        }
        let previous: HashMap<String, String> = transaction
            .prepare(
                "SELECT id, account_id || '|' || platform || '|' || course_id || '|' ||
                        remote_id || '|' || title AS task_key
                 FROM tasks WHERE account_id = ?",
            )?
            .query_map([account_id], |row| Ok((row.get(1)?, row.get(0)?)))?
            .collect::<Result<_, _>>()?;
        let mut current_keys = HashSet::new();
        for task in tasks {
            let platform = string_field(task, "platform", "");
            let course_id = string_field(task, "course_id", "");
            let remote_id = string_field(task, "task_id", "");
            let title = string_field(task, "title", "");
            let key = format!("{account_id}|{platform}|{course_id}|{remote_id}|{title}");
            current_keys.insert(key.clone());
            let task_id = previous
                .get(&key)
                .cloned()
                .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
            transaction.execute(
                "INSERT INTO tasks
                 (id, account_id, platform, course_id, course_name, title, remote_id,
                  status, deadline, url, detail, source, raw, first_seen, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(account_id, platform, course_id, remote_id, title)
                 DO UPDATE SET course_name = excluded.course_name,
                               status = excluded.status, deadline = excluded.deadline,
                               url = excluded.url, detail = excluded.detail,
                               source = excluded.source, raw = excluded.raw,
                               updated_at = excluded.updated_at",
                params![
                    task_id,
                    account_id,
                    platform,
                    course_id,
                    string_field(task, "course_name", ""),
                    title,
                    remote_id,
                    string_field(task, "status", "pending"),
                    task.get("deadline").and_then(Value::as_str),
                    string_field(task, "url", ""),
                    string_field(task, "detail", ""),
                    string_field(task, "source", "task-list"),
                    string_field(task, "raw", "{}"),
                    timestamp,
                    timestamp,
                ],
            )?;
        }
        for (key, task_id) in previous {
            if !current_keys.contains(&key) {
                transaction.execute("DELETE FROM tasks WHERE id = ?", [task_id])?;
            }
        }
        transaction.commit()?;
        Ok(())
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

    #[cfg(test)]
    pub fn upsert_task(&self, account_id: &str, task: &Value) -> anyhow::Result<String> {
        let timestamp = now();
        let platform = string_field(task, "platform", "");
        let course_id = string_field(task, "course_id", "");
        let remote_id = string_field(task, "task_id", "");
        let title = string_field(task, "title", "");
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
                           status = excluded.status, deadline = excluded.deadline,
                           url = excluded.url, detail = excluded.detail,
                           source = excluded.source, raw = excluded.raw,
                           updated_at = excluded.updated_at",
            params![
                task_id,
                account_id,
                platform,
                course_id,
                string_field(task, "course_name", ""),
                title,
                remote_id,
                string_field(task, "status", "pending"),
                task.get("deadline").and_then(Value::as_str),
                string_field(task, "url", ""),
                string_field(task, "detail", ""),
                string_field(task, "source", "essay-lookup"),
                string_field(task, "raw", "{}"),
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
                    title = excluded.title, deadline = excluded.deadline,
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

    // Credentials remain owned by the Python adapter until the secure Rust
    // credential bridge is implemented.
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

    fn sample_task(title: &str) -> Value {
        json!({
            "platform": "pigai",
            "course_id": "pigai:all",
            "course_name": "批改网作业",
            "title": title,
            "task_id": "1",
            "status": "pending",
            "deadline": "2026-11-11 23:59",
            "url": "https://www.pigai.org/",
            "detail": "作文",
            "source": "wait-complete",
            "raw": {}
        })
    }

    #[test]
    fn account_and_task_cache_lifecycle() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let account = store
            .add_legacy_account("pigai", "tester", "test-password", "测试账号")
            .unwrap();
        let id = account["id"].as_str().unwrap();
        assert_eq!(account["username"], "tester");
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
        let id = account["id"].as_str().unwrap();
        let task_id = store.upsert_task(id, &sample_task("第1号 作文")).unwrap();
        assert!(store.update_task_status(&task_id, "completed").unwrap());
        let completed = store.list_completed_tasks(7).unwrap();
        assert_eq!(completed[0]["title"], "第1号 作文");
        assert!(store.update_task_status(&task_id, "pending").unwrap());
        assert!(store.list_completed_tasks(7).unwrap().is_empty());
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
}
