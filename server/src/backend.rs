use std::collections::HashMap;

use chrono::{Duration, Local};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    adapters::execution_mode,
    application::{RefreshSummary, StudyPilotService},
    domain::Account,
    open_targets,
    reminders::parse_reminder_time,
    storage::Store,
};

pub const PLATFORM_LABELS: [(&str, &str); 4] = [
    ("unipus", "U校园"),
    ("welearn", "WELearn"),
    ("chaoxing", "学习通"),
    ("pigai", "批改网"),
];

#[derive(Clone)]
pub struct StudyPilotBackend {
    store: Store,
    service: StudyPilotService,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RefreshAllItem {
    pub account_id: String,
    pub platform: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub courses: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AccountInput {
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TaskReminderInput {
    #[serde(default = "default_reminder_mode")]
    pub mode: String,
    pub remind_at: Option<String>,
    #[serde(default = "default_offset_minutes")]
    pub offset_minutes: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReminderInput {
    pub title: String,
    pub remind_at: String,
    pub account_id: Option<String>,
    pub task_id: Option<String>,
    #[serde(default = "default_repeat_rule")]
    pub repeat_rule: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReminderPatch {
    pub title: Option<String>,
    pub remind_at: Option<String>,
    pub repeat_rule: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Workspace {
    pub platforms: Vec<Value>,
    pub accounts: Vec<Account>,
    pub courses: Vec<Value>,
    pub tasks: Vec<Value>,
    pub completed: Vec<Value>,
    pub reminders: Vec<Value>,
    pub dashboard: Value,
}

fn default_reminder_mode() -> String {
    "countdown".to_owned()
}

fn default_offset_minutes() -> i64 {
    60
}

fn default_repeat_rule() -> String {
    "once".to_owned()
}

pub fn platform_label(platform: &str) -> &str {
    PLATFORM_LABELS
        .iter()
        .find_map(|(id, label)| (*id == platform).then_some(*label))
        .unwrap_or(platform)
}

fn normalize_reminder_time(
    mode: &str,
    remind_at: Option<&str>,
    deadline: Option<&str>,
    minutes_before: i64,
) -> anyhow::Result<String> {
    let result = match mode {
        "absolute" => {
            let value = remind_at
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("请选择提醒日期和时间"))?;
            parse_reminder_time(value)?
        }
        "countdown" => {
            let deadline = deadline
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("该任务没有截止时间，无法设置倒计时提醒"))?;
            anyhow::ensure!(minutes_before >= 0, "倒计时不能为负数");
            parse_reminder_time(deadline)? - Duration::minutes(minutes_before)
        }
        _ => anyhow::bail!("未知的提醒模式"),
    };
    anyhow::ensure!(
        result > Local::now().naive_local(),
        if mode == "countdown" {
            "按该倒计时计算出的提醒时间已经过去"
        } else {
            "提醒时间必须晚于当前时间"
        }
    );
    Ok(result.format("%Y-%m-%dT%H:%M").to_string())
}

fn bump(value: &mut Value, field: &str) {
    value[field] = json!(value[field].as_i64().unwrap_or(0) + 1);
}

impl StudyPilotBackend {
    pub fn new(store: Store) -> Self {
        Self {
            service: StudyPilotService::new(store.clone()),
            store,
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn platforms(&self) -> anyhow::Result<Vec<Value>> {
        let checks = self.store.get_platform_checks()?;
        Ok(PLATFORM_LABELS
            .iter()
            .map(|(id, label)| {
                json!({
                    "id": id,
                    "label": label,
                    "execution_mode": execution_mode(id),
                    "check": checks.get(*id).cloned().unwrap_or_else(|| json!({})),
                })
            })
            .collect())
    }

    pub fn dashboard(&self) -> anyhow::Result<Value> {
        let accounts = self.store.list_accounts()?;
        let tasks = self.store.list_tasks(None, None, None)?;
        let courses = self.store.list_courses(None)?;
        let reminders = self.store.list_reminders()?;
        let pending = tasks
            .iter()
            .filter(|task| task["status"] != "completed")
            .collect::<Vec<_>>();
        let now = Local::now().naive_local();
        let horizon = now + Duration::days(3);
        let due_soon = pending
            .iter()
            .filter(|task| {
                task["deadline"]
                    .as_str()
                    .and_then(|deadline| parse_reminder_time(deadline).ok())
                    .is_some_and(|deadline| now <= deadline && deadline <= horizon)
            })
            .count();

        let mut by_platform: HashMap<String, Value> = HashMap::new();
        for account in &accounts {
            let platform = account.platform.as_str();
            let entry = by_platform.entry(platform.to_owned()).or_insert_with(|| {
                json!({
                    "platform": platform,
                    "label": platform_label(platform),
                    "accounts": 0,
                    "pending": 0,
                    "errors": 0,
                })
            });
            bump(entry, "accounts");
            if account.status == "error" || account.status.ends_with("_error") {
                bump(entry, "errors");
            }
        }
        for task in &pending {
            let platform = task["platform"].as_str().unwrap_or("");
            let entry = by_platform.entry(platform.to_owned()).or_insert_with(|| {
                json!({
                    "platform": platform,
                    "label": platform_label(platform),
                    "accounts": 0,
                    "pending": 0,
                    "errors": 0,
                })
            });
            bump(entry, "pending");
        }
        let mut platform_values = by_platform.into_values().collect::<Vec<_>>();
        platform_values.sort_by(|left, right| {
            left["platform"]
                .as_str()
                .unwrap_or("")
                .cmp(right["platform"].as_str().unwrap_or(""))
        });
        Ok(json!({
            "accounts": accounts.len(),
            "courses": courses.len(),
            "pending": pending.len(),
            "due_soon": due_soon,
            "reminders": reminders.iter().filter(|reminder| reminder["enabled"] == true).count(),
            "platforms": platform_values,
            "platform_checks": self.store.get_platform_checks()?,
            "next_tasks": pending.iter().take(8).collect::<Vec<_>>(),
            "upcoming_reminders": reminders.iter().take(6).collect::<Vec<_>>(),
        }))
    }

    pub fn workspace(&self) -> anyhow::Result<Workspace> {
        Ok(Workspace {
            platforms: self.platforms()?,
            accounts: self.list_accounts()?,
            courses: self.list_courses(None)?,
            tasks: self.list_tasks(None, None, None)?,
            completed: self.list_completed_tasks(7)?,
            reminders: self.list_reminders()?,
            dashboard: self.dashboard()?,
        })
    }

    pub fn list_accounts(&self) -> anyhow::Result<Vec<Account>> {
        self.store.list_accounts()
    }

    pub fn add_account(&self, input: &AccountInput) -> anyhow::Result<Account> {
        let platform = input.platform.as_str();
        anyhow::ensure!(
            PLATFORM_LABELS.iter().any(|(known, _)| *known == platform),
            "不支持的平台：{platform}"
        );
        self.store.add_legacy_account(
            platform,
            input.username.trim(),
            &input.password,
            input.label.trim(),
        )
    }

    pub fn delete_account(&self, account_id: &str) -> anyhow::Result<()> {
        anyhow::ensure!(self.store.delete_account(account_id)?, "账号不存在");
        Ok(())
    }

    pub async fn refresh_account(&self, account_id: &str) -> anyhow::Result<RefreshSummary> {
        self.service
            .refresh_account(account_id)
            .await
            .map_err(|error| anyhow::anyhow!(error))
    }

    pub async fn refresh_all(&self) -> anyhow::Result<Vec<RefreshAllItem>> {
        let accounts = self.store.list_accounts()?;
        let mut results = Vec::with_capacity(accounts.len());
        for account in accounts {
            match self.service.refresh_account(&account.id).await {
                Ok(refreshed) => results.push(RefreshAllItem {
                    account_id: account.id,
                    platform: account.platform,
                    ok: true,
                    tasks: Some(refreshed.tasks),
                    courses: Some(refreshed.courses),
                    warning: Some(refreshed.warning),
                    error: None,
                }),
                Err(error) => results.push(RefreshAllItem {
                    account_id: account.id,
                    platform: account.platform,
                    ok: false,
                    tasks: None,
                    courses: None,
                    warning: None,
                    error: Some(error.to_string()),
                }),
            }
        }
        Ok(results)
    }

    pub async fn lookup_pigai_essay(
        &self,
        account_id: &str,
        essay_id: &str,
    ) -> anyhow::Result<Value> {
        let task = self
            .service
            .lookup_pigai_essay(account_id, essay_id.trim())
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
        Ok(serde_json::to_value(task)?)
    }

    pub fn update_task_status(&self, task_id: &str, status: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            ["pending", "completed", "ignored"].contains(&status),
            "无效状态"
        );
        anyhow::ensure!(
            self.store.update_task_status(task_id, status)?,
            "任务不存在"
        );
        Ok(())
    }

    pub fn restore_task(&self, task_id: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.store.update_task_status(task_id, "pending")?,
            "已完成任务不存在"
        );
        Ok(())
    }

    pub fn list_tasks(
        &self,
        account_id: Option<&str>,
        status: Option<&str>,
        platform: Option<&str>,
    ) -> anyhow::Result<Vec<Value>> {
        self.store.list_tasks(account_id, status, platform)
    }

    pub fn list_courses(&self, account_id: Option<&str>) -> anyhow::Result<Vec<Value>> {
        self.store.list_courses(account_id)
    }

    pub fn list_completed_tasks(&self, days: i64) -> anyhow::Result<Vec<Value>> {
        self.store.list_completed_tasks(days)
    }

    pub fn list_reminders(&self) -> anyhow::Result<Vec<Value>> {
        self.store.list_reminders()
    }

    pub fn delete_reminder(&self, reminder_id: &str) -> anyhow::Result<()> {
        anyhow::ensure!(self.store.delete_reminder(reminder_id)?, "提醒不存在");
        Ok(())
    }

    pub fn open_task(
        &self,
        account_id: &str,
        task_id: &str,
        mobile: bool,
    ) -> anyhow::Result<Value> {
        let account = self
            .store
            .get_account(account_id)?
            .ok_or_else(|| anyhow::anyhow!("账号不存在"))?;
        let task = self
            .store
            .get_task(task_id)?
            .filter(|task| task["account_id"] == account_id)
            .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;
        Ok(serde_json::to_value(open_targets::task_open_target(
            &account.platform,
            task_id,
            task["title"].as_str().unwrap_or(""),
            task["url"].as_str().unwrap_or(""),
            mobile,
        ))?)
    }

    pub fn task_open_target(
        &self,
        account_id: &str,
        task_id: &str,
        mobile: bool,
    ) -> anyhow::Result<open_targets::TaskOpenTarget> {
        let account = self
            .store
            .get_account(account_id)?
            .ok_or_else(|| anyhow::anyhow!("账号不存在"))?;
        let task = self
            .store
            .get_task(task_id)?
            .filter(|task| task["account_id"] == account_id)
            .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;
        Ok(open_targets::task_open_target(
            &account.platform,
            task_id,
            task["title"].as_str().unwrap_or(""),
            task["url"].as_str().unwrap_or(""),
            mobile,
        ))
    }

    pub fn create_task_reminder(
        &self,
        task_id: &str,
        input: &TaskReminderInput,
    ) -> anyhow::Result<Value> {
        let task = self
            .store
            .get_task(task_id)?
            .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;
        let remind_at = normalize_reminder_time(
            &input.mode,
            input.remind_at.as_deref(),
            task["deadline"].as_str(),
            input.offset_minutes,
        )?;
        let title = format!(
            "{}：{}",
            platform_label(task["platform"].as_str().unwrap_or("")),
            task["title"].as_str().unwrap_or("")
        );
        self.store.add_reminder(
            &title,
            &remind_at,
            task["account_id"].as_str(),
            Some(task_id),
            "once",
        )
    }

    pub fn create_reminder(&self, input: &ReminderInput) -> anyhow::Result<Value> {
        let remind_at = input.remind_at.as_str();
        normalize_reminder_time("absolute", Some(remind_at), None, 0)?;
        let repeat_rule = input.repeat_rule.as_str();
        anyhow::ensure!(
            ["once", "daily", "weekdays", "weekly"].contains(&repeat_rule),
            "无效的重复规则"
        );
        self.store.add_reminder(
            &input.title,
            remind_at,
            input.account_id.as_deref(),
            input.task_id.as_deref(),
            repeat_rule,
        )
    }

    pub fn update_reminder(
        &self,
        reminder_id: &str,
        input: &ReminderPatch,
    ) -> anyhow::Result<Value> {
        let mut fields = serde_json::Map::new();
        if let Some(title) = &input.title {
            fields.insert("title".to_owned(), json!(title));
        }
        if let Some(remind_at) = &input.remind_at {
            normalize_reminder_time("absolute", Some(remind_at), None, 0)?;
            fields.insert("remind_at".to_owned(), json!(remind_at));
        }
        if let Some(repeat_rule) = &input.repeat_rule {
            anyhow::ensure!(
                ["once", "daily", "weekdays", "weekly"].contains(&repeat_rule.as_str()),
                "无效的重复规则"
            );
            fields.insert("repeat_rule".to_owned(), json!(repeat_rule));
        }
        if let Some(enabled) = input.enabled {
            fields.insert("enabled".to_owned(), json!(enabled));
        }
        anyhow::ensure!(
            self.store
                .update_reminder(reminder_id, &Value::Object(fields))?,
            "提醒不存在"
        );
        self.store
            .get_reminder(reminder_id)?
            .ok_or_else(|| anyhow::anyhow!("提醒不存在"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StoreInner;

    #[test]
    fn create_task_reminder_uses_shared_timestamp_rules() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let account = store
            .add_legacy_account("pigai", "tester", "password", "")
            .unwrap();
        let task_id =
            crate::storage::create_test_task(&store, &account.id, "1", "第一号 作文").unwrap();
        let backend = StudyPilotBackend::new(store);
        let reminder = backend
            .create_task_reminder(
                &task_id,
                &TaskReminderInput {
                    mode: "countdown".to_owned(),
                    remind_at: None,
                    offset_minutes: 10,
                },
            )
            .unwrap();
        assert_eq!(reminder["remind_at"], "2099-12-31T23:49");
        assert_eq!(reminder["repeat_rule"], "once");
        assert_eq!(reminder["account_id"], account.id);
    }

    #[test]
    fn platform_metadata_exposes_adapter_execution_mode() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let platforms = StudyPilotBackend::new(store).platforms().unwrap();
        let mode = |platform: &str| {
            platforms
                .iter()
                .find(|item| item["id"] == platform)
                .unwrap()["execution_mode"]
                .clone()
        };
        assert_eq!(mode("pigai"), "native-rust");
        assert_eq!(mode("unipus"), "python-compatibility");
        assert_eq!(mode("welearn"), "python-compatibility");
        assert_eq!(mode("chaoxing"), "python-compatibility");
    }
}
