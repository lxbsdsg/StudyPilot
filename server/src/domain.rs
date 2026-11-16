//! API-facing domain types with stable serialized field names.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Account {
    pub id: String,
    pub platform: String,
    pub username: String,
    #[serde(default)]
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<Vec<u8>>,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub school: String,
    #[serde(default)]
    pub session: serde_json::Value,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_error: String,
    #[serde(default)]
    pub last_refresh: i64,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AccountInfo {
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub school: String,
    #[serde(default)]
    pub extra: serde_json::Value,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Course {
    pub account_id: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub progress: i64,
    #[serde(default)]
    pub raw: serde_json::Value,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Task {
    pub id: String,
    pub account_id: String,
    pub platform: String,
    pub course_id: String,
    #[serde(default)]
    pub course_name: String,
    pub title: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub raw: serde_json::Value,
    #[serde(default)]
    pub first_seen: i64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Completed,
    Ignored,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Completed => "completed",
            Self::Ignored => "ignored",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatRule {
    Once,
    Daily,
    Weekdays,
    Weekly,
}

impl RepeatRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Daily => "daily",
            Self::Weekdays => "weekdays",
            Self::Weekly => "weekly",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CourseSnapshot {
    pub platform: String,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub progress: i64,
    #[serde(default)]
    pub raw: serde_json::Value,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TaskSnapshot {
    pub platform: String,
    pub course_id: String,
    #[serde(default)]
    pub course_name: String,
    pub title: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub raw: serde_json::Value,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RefreshResult {
    #[serde(default)]
    pub courses: Vec<CourseSnapshot>,
    #[serde(default)]
    pub tasks: Vec<TaskSnapshot>,
    #[serde(default)]
    pub account: Option<AccountInfo>,
    #[serde(default)]
    pub warning: String,
    #[serde(default)]
    pub session: serde_json::Value,
}
