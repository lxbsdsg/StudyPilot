mod config;
mod open_targets;
mod parse_utils;
pub mod security;
mod storage;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{
        HeaderMap, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE, LOCATION},
    },
    response::{Html, IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use storage::Store;
#[cfg(test)]
use tower::ServiceExt;
use tracing::info;

use crate::{
    config::{PLATFORM_LABELS, VERSION, platform_label},
    storage::StoreInner,
};

#[derive(Clone)]
struct AppState {
    store: Store,
    web_dir: Arc<PathBuf>,
}

#[derive(Debug)]
struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("request failed: {:#}", self.0);
        let status = if self.0.to_string().ends_with("不存在") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::BAD_REQUEST
        };
        (status, Json(json!({"detail": self.0.to_string()}))).into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

type ApiResult<T> = Result<Json<T>, AppError>;

#[derive(Debug, Deserialize)]
struct AccountCreate {
    #[serde(default)]
    platform: String,
    #[serde(default)]
    username: String,
    // Accepted for API compatibility; credentials are delegated to the Python bridge.
    #[allow(dead_code)]
    password: Option<String>,
    #[serde(default)]
    label: String,
}

#[derive(Debug, Deserialize)]
struct ReminderCreate {
    title: String,
    remind_at: String,
    account_id: Option<String>,
    task_id: Option<String>,
    #[serde(default = "once_rule")]
    repeat_rule: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReminderPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remind_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_rule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct TaskStatus {
    status: String,
}

#[derive(Debug, Deserialize)]
struct TaskReminder {
    #[serde(default = "countdown_mode")]
    mode: String,
    remind_at: Option<String>,
    #[serde(default = "hour_offset")]
    offset_minutes: i64,
}

#[derive(Debug, Deserialize)]
struct OpenRequest {
    account_id: String,
    task_id: String,
    #[serde(default)]
    mobile: bool,
}

#[derive(Debug, Deserialize)]
struct TaskOpenQuery {
    #[allow(dead_code)]
    account_id: Option<String>,
    #[allow(dead_code)]
    task_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ParseDeadlineRequest {
    text: String,
}

#[derive(Debug, Deserialize)]
struct ListTaskQuery {
    account_id: Option<String>,
    status: Option<String>,
    platform: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CourseQuery {
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CompletedQuery {
    #[serde(default = "seven_days")]
    days: i64,
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    version: &'static str,
    runtime: &'static str,
}

fn once_rule() -> String {
    "once".to_owned()
}

fn countdown_mode() -> String {
    "countdown".to_owned()
}

fn hour_offset() -> i64 {
    60
}

fn seven_days() -> i64 {
    7
}

fn parse_reminder_time(value: &str) -> anyhow::Result<NaiveDateTime> {
    for format in ["%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(value) = NaiveDateTime::parse_from_str(value.trim(), format) {
            return Ok(value);
        }
    }
    anyhow::bail!("提醒时间格式无效")
}

fn normalize_reminder_time(
    mode: &str,
    remind_at: Option<&str>,
    deadline: Option<&str>,
    minutes_before: i64,
) -> anyhow::Result<String> {
    let now = Local::now().naive_local();
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
            if minutes_before < 0 {
                anyhow::bail!("倒计时不能为负数");
            }
            parse_reminder_time(deadline)? - chrono::Duration::minutes(minutes_before)
        }
        _ => anyhow::bail!("未知的提醒模式"),
    };
    if result <= now {
        anyhow::bail!(if mode == "countdown" {
            "按该倒计时计算出的提醒时间已经过去"
        } else {
            "提醒时间必须晚于当前时间"
        });
    }
    Ok(result.format("%Y-%m-%dT%H:%M").to_string())
}

async fn index(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let path = state.web_dir.join("index.html");
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|error| anyhow::anyhow!("无法读取前端 {}: {error}", path.display()))?;
    Ok(Html(content))
}

async fn static_file(
    State(state): State<AppState>,
    Path(file): Path<String>,
) -> Result<Response, AppError> {
    if file.is_empty() || file.contains('/') || file.contains('\\') || file == "." || file == ".." {
        return Err(anyhow::anyhow!("静态文件不存在").into());
    }
    let path = state.web_dir.join(&file);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| anyhow::anyhow!("静态文件不存在"))?;
    let content_type = match path.extension().and_then(|ext| ext.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    };
    Ok((
        [(CONTENT_TYPE, content_type), (CACHE_CONTROL, "no-cache")],
        bytes,
    )
        .into_response())
}

async fn health() -> Json<Health> {
    Json(Health {
        ok: true,
        version: VERSION,
        runtime: "rust",
    })
}

async fn platforms(State(state): State<AppState>) -> ApiResult<Vec<Value>> {
    let checks = state.store.get_platform_checks()?;
    Ok(Json(
        PLATFORM_LABELS
            .iter()
            .map(|(id, label)| {
                json!({
                    "id": id,
                    "label": label,
                    "check": checks.get(*id).cloned().unwrap_or_else(|| json!({})),
                })
            })
            .collect(),
    ))
}

async fn dashboard(State(state): State<AppState>) -> ApiResult<Value> {
    let accounts = state.store.list_accounts()?;
    let tasks = state.store.list_tasks(None, None, None)?;
    let courses = state.store.list_courses(None)?;
    let reminders = state.store.list_reminders()?;
    let pending: Vec<Value> = tasks
        .iter()
        .filter(|task| task["status"] != "completed")
        .cloned()
        .collect();
    let now = Local::now().naive_local();
    let horizon = now + chrono::Duration::days(3);
    let due_soon = pending
        .iter()
        .filter(|task| {
            task["deadline"]
                .as_str()
                .and_then(|deadline| parse_reminder_time(deadline).ok())
                .is_some_and(|deadline| now <= deadline && deadline <= horizon)
        })
        .count();
    let mut by_platform = serde_json::Map::new();
    for account in &accounts {
        let platform = account["platform"].as_str().unwrap_or("");
        let entry = by_platform.entry(platform.to_owned()).or_insert_with(|| {
            json!({
                "platform": platform,
                "label": platform_label(platform),
                "accounts": 0,
                "pending": 0,
                "errors": 0,
            })
        });
        entry["accounts"] = json!(entry["accounts"].as_i64().unwrap_or(0) + 1);
        if account["status"] == "error" {
            entry["errors"] = json!(entry["errors"].as_i64().unwrap_or(0) + 1);
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
        entry["pending"] = json!(entry["pending"].as_i64().unwrap_or(0) + 1);
    }
    Ok(Json(json!({
        "accounts": accounts.len(),
        "courses": courses.len(),
        "pending": pending.len(),
        "due_soon": due_soon,
        "reminders": reminders.iter().filter(|reminder| reminder["enabled"] == true).count(),
        "platforms": by_platform.values().collect::<Vec<_>>(),
        "platform_checks": state.store.get_platform_checks()?,
        "next_tasks": pending.iter().take(8).collect::<Vec<_>>(),
        "upcoming_reminders": reminders.iter().take(6).collect::<Vec<_>>(),
    })))
}

async fn accounts(State(state): State<AppState>) -> ApiResult<Vec<Value>> {
    Ok(Json(state.store.list_accounts()?))
}

async fn create_account(
    State(state): State<AppState>,
    Json(payload): Json<AccountCreate>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    if !PLATFORM_LABELS
        .iter()
        .any(|(platform, _)| *platform == payload.platform)
    {
        return Err(anyhow::anyhow!("不支持的平台：{}", payload.platform).into());
    }
    let account = state.store.add_legacy_account(
        &payload.platform,
        payload.username.trim(),
        payload.password.as_deref().unwrap_or(""),
        payload.label.trim(),
    )?;
    Ok((StatusCode::CREATED, Json(account)))
}

async fn delete_account(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
) -> Result<StatusCode, AppError> {
    if !state.store.delete_account(&account_id)? {
        return Err(anyhow::anyhow!("账号不存在").into());
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn deferred_refresh() -> Result<Json<Value>, AppError> {
    Err(anyhow::anyhow!("Rust 平台适配器尚未迁移，请暂时使用 Python 服务刷新账号").into())
}

async fn deferred_refresh_all() -> Result<Json<Vec<Value>>, AppError> {
    Err(anyhow::anyhow!("Rust 平台适配器尚未迁移，请暂时使用 Python 服务刷新账号").into())
}

async fn deferred_pigai_lookup() -> Result<Json<Value>, AppError> {
    Err(anyhow::anyhow!("Rust 批改网适配器尚未迁移，请暂时使用 Python 服务查询作文").into())
}

async fn events() -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn parse_deadline(Json(payload): Json<ParseDeadlineRequest>) -> Json<Value> {
    Json(json!({"deadline": parse_utils::parse_deadline(&payload.text)}))
}

async fn platform_check(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Map<String, Value>> {
    Ok(Json(state.store.get_platform_checks()?))
}

async fn courses(
    State(state): State<AppState>,
    Query(query): Query<CourseQuery>,
) -> ApiResult<Vec<Value>> {
    Ok(Json(state.store.list_courses(query.account_id.as_deref())?))
}

async fn tasks(
    State(state): State<AppState>,
    Query(query): Query<ListTaskQuery>,
) -> ApiResult<Vec<Value>> {
    Ok(Json(state.store.list_tasks(
        query.account_id.as_deref(),
        query.status.as_deref(),
        query.platform.as_deref(),
    )?))
}

async fn completed_tasks(
    State(state): State<AppState>,
    Query(query): Query<CompletedQuery>,
) -> ApiResult<Vec<Value>> {
    Ok(Json(state.store.list_completed_tasks(query.days)?))
}

async fn open_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<OpenRequest>,
) -> ApiResult<Value> {
    let account = state
        .store
        .get_account(&payload.account_id)?
        .ok_or_else(|| anyhow::anyhow!("账号不存在"))?;
    let task = state
        .store
        .get_task(&payload.task_id)?
        .filter(|task| task["account_id"] == payload.account_id)
        .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;
    let mobile = payload.mobile
        || headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .is_some_and(open_targets::is_mobile_user_agent);
    let target = open_targets::task_open_target(
        account["platform"].as_str().unwrap_or(""),
        &payload.task_id,
        task["title"].as_str().unwrap_or(""),
        task["url"].as_str().unwrap_or(""),
        mobile,
    );
    Ok(Json(serde_json::to_value(target)?))
}

async fn open_link(
    State(state): State<AppState>,
    Path((account_id, task_id)): Path<(String, String)>,
    _query: Query<TaskOpenQuery>,
) -> Result<(StatusCode, [(axum::http::HeaderName, String); 1]), AppError> {
    let account = state
        .store
        .get_account(&account_id)?
        .ok_or_else(|| anyhow::anyhow!("账号不存在"))?;
    let task = state
        .store
        .get_task(&task_id)?
        .filter(|task| task["account_id"] == account_id)
        .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;
    let url = open_targets::desktop_url(
        account["platform"].as_str().unwrap_or(""),
        task["url"].as_str().unwrap_or(""),
    );
    Ok((StatusCode::TEMPORARY_REDIRECT, [(LOCATION, url)]))
}

async fn update_task_status(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(payload): Json<TaskStatus>,
) -> Result<Json<Value>, AppError> {
    if !["pending", "completed", "ignored"].contains(&payload.status.as_str()) {
        return Err(anyhow::anyhow!("无效状态").into());
    }
    if !state.store.update_task_status(&task_id, &payload.status)? {
        return Err(anyhow::anyhow!("任务不存在").into());
    }
    Ok(Json(json!({"ok": true})))
}

async fn restore_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    if !state.store.update_task_status(&task_id, "pending")? {
        return Err(anyhow::anyhow!("已完成任务不存在").into());
    }
    Ok(Json(json!({"ok": true})))
}

async fn create_task_reminder(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(payload): Json<TaskReminder>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let task = state
        .store
        .get_task(&task_id)?
        .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;
    let remind_at = normalize_reminder_time(
        &payload.mode,
        payload.remind_at.as_deref(),
        task["deadline"].as_str(),
        payload.offset_minutes,
    )?;
    let title = format!(
        "{}：{}",
        platform_label(task["platform"].as_str().unwrap_or("")),
        task["title"].as_str().unwrap_or("")
    );
    let reminder = state.store.add_reminder(
        &title,
        &remind_at,
        task["account_id"].as_str(),
        Some(&task_id),
        "once",
    )?;
    Ok((StatusCode::CREATED, Json(reminder)))
}

async fn reminders(State(state): State<AppState>) -> ApiResult<Vec<Value>> {
    Ok(Json(state.store.list_reminders()?))
}

async fn create_reminder(
    State(state): State<AppState>,
    Json(payload): Json<ReminderCreate>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    normalize_reminder_time("absolute", Some(&payload.remind_at), None, 0)?;
    if !["once", "daily", "weekdays", "weekly"].contains(&payload.repeat_rule.as_str()) {
        return Err(anyhow::anyhow!("无效的重复规则").into());
    }
    let reminder = state.store.add_reminder(
        &payload.title,
        &payload.remind_at,
        payload.account_id.as_deref(),
        payload.task_id.as_deref(),
        &payload.repeat_rule,
    )?;
    Ok((StatusCode::CREATED, Json(reminder)))
}

async fn update_reminder(
    State(state): State<AppState>,
    Path(reminder_id): Path<String>,
    Json(payload): Json<ReminderPatch>,
) -> ApiResult<Value> {
    let fields = serde_json::to_value(payload)?;
    if !state.store.update_reminder(&reminder_id, &fields)? {
        return Err(anyhow::anyhow!("提醒不存在").into());
    }
    Ok(Json(
        state
            .store
            .get_reminder(&reminder_id)?
            .ok_or_else(|| anyhow::anyhow!("提醒不存在"))?,
    ))
}

async fn delete_reminder(
    State(state): State<AppState>,
    Path(reminder_id): Path<String>,
) -> Result<StatusCode, AppError> {
    if !state.store.delete_reminder(&reminder_id)? {
        return Err(anyhow::anyhow!("提醒不存在").into());
    }
    Ok(StatusCode::NO_CONTENT)
}

fn seed_platform_checks(store: &StoreInner) -> anyhow::Result<()> {
    let checks = store.get_platform_checks()?;
    let defaults = [
        ("pigai", "verified", "已用测试账号验证登录与待完成列表读取"),
        (
            "unipus",
            "endpoint_checked",
            "登录端点已在线验证，等待真实账号完成课程抓取",
        ),
        (
            "chaoxing",
            "endpoint_checked",
            "登录端点已在线验证，等待真实账号完成课程与作业抓取",
        ),
        (
            "welearn",
            "needs_account",
            "SSO 已改版为浏览器流程，等待真实账号验证",
        ),
    ];
    for (platform, status, note) in defaults {
        if !checks.contains_key(platform) {
            store.set_platform_check(platform, status, note)?;
        }
    }
    Ok(())
}

fn web_dir() -> PathBuf {
    std::env::var_os("STUDYPILOT_WEB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            manifest
                .join("../../studypilot/web")
                .canonicalize()
                .or_else(|_| manifest.join("../../../studypilot/web").canonicalize())
                .unwrap_or_else(|_| PathBuf::from("studypilot/web"))
        })
}

fn app(state: AppState) -> Router {
    Router::new()
        .fallback(not_found_route)
        .route("/", get(index))
        .route("/static/{file}", get(static_file))
        .route("/api/health", get(health))
        .route("/api/platforms", get(platforms))
        .route("/api/dashboard", get(dashboard))
        .route("/api/events", get(events))
        .route("/api/platform-checks", get(platform_check))
        .route("/api/parse/deadline", post(parse_deadline))
        .route("/api/accounts", get(accounts).post(create_account))
        .route("/api/accounts/refresh", post(deferred_refresh_all))
        .route("/api/accounts/{account_id}", delete(delete_account))
        .route("/api/accounts/{account_id}/refresh", post(deferred_refresh))
        .route("/api/courses", get(courses))
        .route("/api/tasks", get(tasks))
        .route("/api/tasks/{task_id}/status", post(update_task_status))
        .route("/api/tasks/{task_id}/reminder", post(create_task_reminder))
        .route("/api/completed-tasks", get(completed_tasks))
        .route("/api/completed-tasks/{task_id}/restore", post(restore_task))
        .route("/api/pigai/essay", post(deferred_pigai_lookup))
        .route("/api/open", post(open_task))
        .route("/open/{account_id}/{task_id}", get(open_link))
        .route("/api/reminders", get(reminders).post(create_reminder))
        .route(
            "/api/reminders/{reminder_id}",
            patch(update_reminder).delete(delete_reminder),
        )
        .with_state(state)
        .layer(DefaultBodyLimit::max(64 * 1024))
}

async fn not_found_route() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({"detail": "资源不存在"}))).into_response()
}

fn pick_port(host: &str, preferred: u16) -> u16 {
    for port in std::iter::once(preferred).chain(config::DEFAULT_PORT..config::PORT_END) {
        if std::net::TcpListener::bind((host, port)).is_ok() {
            return port;
        }
    }
    0
}

fn open_browser(url: &str) {
    let mut command = if cfg!(windows) {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };
    if let Err(error) = command.spawn() {
        tracing::warn!("cannot open browser: {error}");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "studypilot_server=info".into()),
        )
        .init();
    let config = config::Config::from_env()?;
    let store = StoreInner::open(config.data_dir.join("studypilot.db"))?;
    seed_platform_checks(&store)?;
    let state = AppState {
        store,
        web_dir: Arc::new(web_dir()),
    };
    let preferred_port = pick_port(&config.host, config.port);
    let listener = tokio::net::TcpListener::bind((config.host.as_str(), preferred_port)).await?;
    let port = listener.local_addr()?.port();
    let url = format!("http://{}:{port}/", config.host);
    info!("StudyPilot Rust server listening at {url}");
    if config.open_browser {
        let browser_url = url.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(800));
            open_browser(&browser_url);
        });
    }
    axum::serve(listener, app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Method, Request},
    };

    async fn response_body(response: Response) -> Value {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn api_supports_frontend_workflows_without_exposing_password() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let web_dir = web_dir();
        assert!(web_dir.join("index.html").is_file());
        let state = AppState {
            store: store.clone(),
            web_dir: Arc::new(web_dir),
        };
        let app = app(state);
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/accounts")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"platform":"pigai","username":"api-test","password":"secret","label":"测试"}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let account = response_body(response).await;
        assert!(account.get("password").is_none());
        let account_id = account["id"].as_str().unwrap();
        assert_eq!(
            security::unprotect_password(
                &store.credential_bytes(account_id).unwrap(),
                &store.secret_file()
            )
            .unwrap(),
            "secret"
        );
        assert_eq!(
            store.account_session(account_id).unwrap(),
            Some(json!({}))
        );

        let task_id = storage::create_test_task(&store, account_id, "1", "第一号 作文").unwrap();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/tasks/".to_owned() + &task_id + "/status")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"status":"completed"}"#))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = Request::builder()
            .uri("/api/tasks/".to_owned() + &task_id + "/status")
            .method(Method::POST)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"status":"completed"}"#))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = Request::builder()
            .uri("/api/completed-tasks?days=7")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let completed = response_body(response).await;
        assert_eq!(completed[0]["title"], "第一号 作文");
        assert_eq!(completed[0]["status"], "completed");

        let request = Request::builder()
            .uri("/api/completed-tasks/".to_owned() + &task_id + "/restore")
            .method(Method::POST)
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let request = Request::builder()
            .uri("/api/tasks/".to_owned() + &task_id + "/reminder")
            .method(Method::POST)
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"mode":"countdown","offset_minutes":10}"#))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let reminder = response_body(response).await;
        assert_eq!(reminder["remind_at"], "2099-12-31T23:49");

        let request = Request::builder()
            .uri("/open/".to_owned() + account_id + "/" + &task_id)
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response.headers().get(LOCATION).unwrap(),
            "https://www.pigai.org/"
        );
    }
}
