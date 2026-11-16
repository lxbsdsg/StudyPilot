use std::{
    path::{Path as FilePath, PathBuf},
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS},
    },
    response::{Html, IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::warn;

use crate::{
    backend::{
        AccountInput, RefreshAllItem, ReminderInput, ReminderPatch, StudyPilotBackend,
        TaskReminderInput,
    },
    config::data_dir,
    domain::Account,
    open_targets, parse_utils,
    storage::{Store, StoreInner},
};

#[derive(Clone)]
pub struct ApiState {
    backend: StudyPilotBackend,
    web_dir: Arc<PathBuf>,
}

#[derive(Debug)]
pub struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        warn!("request failed: {:#}", self.0);
        let status = if self.0.to_string().ends_with("不存在") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::BAD_REQUEST
        };
        let mut response = (status, Json(json!({"detail": self.0.to_string()}))).into_response();
        response
            .headers_mut()
            .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
        response
    }
}

impl<E> From<E> for ApiError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

#[derive(Debug, Deserialize)]
struct TaskStatus {
    status: String,
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

fn seven_days() -> i64 {
    7
}

fn secure_index(content: String) -> Response {
    let mut response = Html(content).into_response();
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; \
         base-uri 'none'; form-action 'self'",
        ),
    );
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn index(State(state): State<ApiState>) -> Result<Response, ApiError> {
    let path = state.web_dir.join("index.html");
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|error| anyhow::anyhow!("无法读取前端 {}: {error}", path.display()))?;
    Ok(secure_index(content))
}

async fn static_file(
    State(state): State<ApiState>,
    Path(file): Path<String>,
) -> Result<Response, ApiError> {
    if file.contains('/')
        || file.contains('\\')
        || file == "."
        || file == ".."
        || !allowed_static_extension(FilePath::new(&file))
    {
        return Err(anyhow::anyhow!("静态文件不存在").into());
    }
    serve_static_file(&state.web_dir, FilePath::new(&file), "no-cache").await
}

async fn asset_file(
    State(state): State<ApiState>,
    Path(path): Path<String>,
) -> Result<Response, ApiError> {
    let segments = path.split('/').collect::<Vec<_>>();
    let relative = FilePath::new(&path);
    if segments.is_empty()
        || segments
            .iter()
            .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
        || relative.is_absolute()
        || path.contains('\\')
        || !allowed_static_extension(relative)
    {
        return Err(anyhow::anyhow!("静态文件不存在").into());
    }
    serve_static_file(
        &state.web_dir,
        &FilePath::new("assets").join(relative),
        "public, max-age=31536000, immutable",
    )
    .await
}

fn allowed_static_extension(path: &FilePath) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("css" | "js" | "html")
    )
}

async fn serve_static_file(
    web_dir: &FilePath,
    relative: &FilePath,
    cache_control: &'static str,
) -> Result<Response, ApiError> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(anyhow::anyhow!("静态文件不存在").into());
    }
    let path = web_dir.join(relative);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| anyhow::anyhow!("静态文件不存在"))?;
    let content_type = match relative.extension().and_then(|ext| ext.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    };
    Ok((
        [
            (CONTENT_TYPE, content_type),
            (CACHE_CONTROL, cache_control),
            (X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response())
}

async fn health() -> Json<Health> {
    Json(Health {
        ok: true,
        version: crate::VERSION,
        runtime: "rust-v2",
    })
}

async fn platforms(State(state): State<ApiState>) -> ApiResult<Vec<Value>> {
    Ok(Json(state.backend.platforms()?))
}

async fn dashboard(State(state): State<ApiState>) -> ApiResult<Value> {
    Ok(Json(state.backend.dashboard()?))
}

async fn workspace(State(state): State<ApiState>) -> Json<crate::backend::Workspace> {
    Json(
        state
            .backend
            .workspace()
            .expect("workspace queries are infallible"),
    )
}

async fn accounts(State(state): State<ApiState>) -> ApiResult<Vec<Account>> {
    Ok(Json(state.backend.list_accounts()?))
}

async fn create_account(
    State(state): State<ApiState>,
    Json(payload): Json<AccountInput>,
) -> Result<(StatusCode, Json<Account>), ApiError> {
    let account = state.backend.add_account(&payload)?;
    Ok((StatusCode::CREATED, Json(account)))
}

async fn delete_account(
    State(state): State<ApiState>,
    Path(account_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.backend.delete_account(&account_id)?;
    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
async fn refresh_account(
    State(state): State<ApiState>,
    Path(account_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let refreshed = state.backend.refresh_account(&account_id).await?;
    Ok(Json(serde_json::to_value(refreshed)?))
}

#[axum::debug_handler]
async fn refresh_all(State(state): State<ApiState>) -> ApiResult<Vec<RefreshAllItem>> {
    Ok(Json(state.backend.refresh_all().await?))
}

#[derive(Debug, Deserialize)]
struct PigaiEssayRequest {
    account_id: String,
    essay_id: String,
}

async fn pigai_essay_lookup(
    State(state): State<ApiState>,
    Json(payload): Json<PigaiEssayRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        state
            .backend
            .lookup_pigai_essay(&payload.account_id, &payload.essay_id)
            .await?,
    ))
}

async fn events() -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn parse_deadline(Json(payload): Json<ParseDeadlineRequest>) -> Json<Value> {
    Json(json!({"deadline": parse_utils::parse_deadline(&payload.text)}))
}

async fn platform_check(
    State(state): State<ApiState>,
) -> ApiResult<serde_json::Map<String, Value>> {
    Ok(Json(state.backend.store().get_platform_checks()?))
}

async fn courses(
    State(state): State<ApiState>,
    Query(query): Query<CourseQuery>,
) -> ApiResult<Vec<Value>> {
    Ok(Json(
        state.backend.list_courses(query.account_id.as_deref())?,
    ))
}

async fn tasks(
    State(state): State<ApiState>,
    Query(query): Query<ListTaskQuery>,
) -> ApiResult<Vec<Value>> {
    Ok(Json(state.backend.list_tasks(
        query.account_id.as_deref(),
        query.status.as_deref(),
        query.platform.as_deref(),
    )?))
}

async fn completed_tasks(
    State(state): State<ApiState>,
    Query(query): Query<CompletedQuery>,
) -> ApiResult<Vec<Value>> {
    Ok(Json(state.backend.list_completed_tasks(query.days)?))
}

async fn open_task(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(payload): Json<OpenRequest>,
) -> ApiResult<Value> {
    let mobile = payload.mobile
        || headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .is_some_and(open_targets::is_mobile_user_agent);
    Ok(Json(state.backend.open_task(
        &payload.account_id,
        &payload.task_id,
        mobile,
    )?))
}

async fn open_link(
    State(state): State<ApiState>,
    Path((account_id, task_id)): Path<(String, String)>,
    _query: Query<TaskOpenQuery>,
) -> Result<(StatusCode, [(axum::http::HeaderName, String); 1]), ApiError> {
    let account = state
        .backend
        .store()
        .get_account(&account_id)?
        .ok_or_else(|| anyhow::anyhow!("账号不存在"))?;
    let task = state
        .backend
        .store()
        .get_task(&task_id)?
        .filter(|task| task["account_id"] == account_id)
        .ok_or_else(|| anyhow::anyhow!("任务不存在"))?;
    let url = open_targets::desktop_url(&account.platform, task["url"].as_str().unwrap_or(""));
    Ok((
        StatusCode::TEMPORARY_REDIRECT,
        [(axum::http::header::LOCATION, url)],
    ))
}

async fn update_task_status(
    State(state): State<ApiState>,
    Path(task_id): Path<String>,
    Json(payload): Json<TaskStatus>,
) -> Result<Json<Value>, ApiError> {
    state
        .backend
        .update_task_status(&task_id, &payload.status)?;
    Ok(Json(json!({"ok": true})))
}

async fn restore_task(
    State(state): State<ApiState>,
    Path(task_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state.backend.restore_task(&task_id)?;
    Ok(Json(json!({"ok": true})))
}

async fn create_task_reminder(
    State(state): State<ApiState>,
    Path(task_id): Path<String>,
    Json(payload): Json<TaskReminderInput>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let reminder = state.backend.create_task_reminder(&task_id, &payload)?;
    Ok((StatusCode::CREATED, Json(reminder)))
}

async fn reminders(State(state): State<ApiState>) -> ApiResult<Vec<Value>> {
    Ok(Json(state.backend.list_reminders()?))
}

async fn create_reminder(
    State(state): State<ApiState>,
    Json(payload): Json<ReminderInput>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let reminder = state.backend.create_reminder(&payload)?;
    Ok((StatusCode::CREATED, Json(reminder)))
}

async fn update_reminder(
    State(state): State<ApiState>,
    Path(reminder_id): Path<String>,
    Json(payload): Json<ReminderPatch>,
) -> ApiResult<Value> {
    Ok(Json(state.backend.update_reminder(&reminder_id, &payload)?))
}

async fn delete_reminder(
    State(state): State<ApiState>,
    Path(reminder_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.backend.delete_reminder(&reminder_id)?;
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
    if let Some(value) = std::env::var_os("STUDYPILOT_WEB_DIR") {
        return PathBuf::from(value);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest.join("../frontend/dist"),
        manifest.join("../studypilot/web"),
    ];
    candidates
        .into_iter()
        .find(|path| path.join("index.html").is_file())
        .unwrap_or_else(|| PathBuf::from("../frontend/dist"))
}

fn route(state: ApiState) -> Router {
    Router::new()
        .fallback(not_found_route)
        .route("/", get(index))
        .route("/assets/{*path}", get(asset_file))
        .route("/static/{file}", get(static_file))
        .route("/api/health", get(health))
        .route("/api/platforms", get(platforms))
        .route("/api/dashboard", get(dashboard))
        .route("/api/workspace", get(workspace))
        .route("/api/events", get(events))
        .route("/api/platform-checks", get(platform_check))
        .route("/api/parse/deadline", post(parse_deadline))
        .route("/api/accounts", get(accounts).post(create_account))
        .route("/api/accounts/refresh", post(refresh_all))
        .route("/api/accounts/{account_id}", delete(delete_account))
        .route("/api/accounts/{account_id}/refresh", post(refresh_account))
        .route("/api/courses", get(courses))
        .route("/api/tasks", get(tasks))
        .route("/api/tasks/{task_id}/status", post(update_task_status))
        .route("/api/tasks/{task_id}/reminder", post(create_task_reminder))
        .route("/api/completed-tasks", get(completed_tasks))
        .route("/api/completed-tasks/{task_id}/restore", post(restore_task))
        .route("/api/pigai/essay", post(pigai_essay_lookup))
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

pub fn open_default_store() -> anyhow::Result<Store> {
    let store = StoreInner::open(data_dir()?.join("studypilot.db"))?;
    seed_platform_checks(&store)?;
    Ok(store)
}

pub fn router(store: Store, web_dir: impl Into<PathBuf>) -> Router {
    route(ApiState {
        backend: StudyPilotBackend::new(store),
        web_dir: Arc::new(web_dir.into()),
    })
}

pub fn default_router(store: Store) -> Router {
    router(store, web_dir())
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Method, Request},
    };
    use tower::ServiceExt;

    use super::*;
    use crate::security;

    async fn body(response: Response) -> Value {
        serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn api_preserves_workflows_and_never_returns_secret() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let web_dir = web_dir();
        assert!(web_dir.join("index.html").is_file());
        let app = router(store.clone(), web_dir);
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
        let account = body(response).await;
        assert!(account.get("secret").is_none());
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
        assert_eq!(store.account_session(account_id).unwrap(), Some(json!({})));

        let task_id =
            crate::storage::create_test_task(&store, account_id, "1", "第一号 作文").unwrap();
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/tasks/{task_id}/status"))
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"status":"completed"}"#))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::OK
        );
        let request = Request::builder()
            .uri("/api/completed-tasks?days=7")
            .body(Body::empty())
            .unwrap();
        let completed = body(app.clone().oneshot(request).await.unwrap()).await;
        assert_eq!(completed[0]["title"], "第一号 作文");

        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/tasks/{task_id}/reminder"))
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"mode":"countdown","offset_minutes":10}"#))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let reminder = body(response).await;
        assert_eq!(reminder["remind_at"], "2099-12-31T23:49");

        let request = Request::builder()
            .uri(format!("/open/{account_id}/{task_id}"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::LOCATION)
                .unwrap(),
            "https://www.pigai.org/"
        );

        let request = Request::builder()
            .uri("/api/accounts")
            .body(Body::empty())
            .unwrap();
        let accounts = body(app.clone().oneshot(request).await.unwrap()).await;
        assert!(accounts[0].get("secret").is_none());
        assert!(accounts[0].get("password").is_none());

        for file in ["../Cargo.toml", "Cargo.toml", "index.html", "style.css"] {
            let expected = if file == "index.html" {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            };
            let request = Request::builder()
                .uri(format!("/static/{file}"))
                .body(Body::empty())
                .unwrap();
            assert_eq!(
                app.clone().oneshot(request).await.unwrap().status(),
                expected,
                "{file}"
            );
        }

        let frontend_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend/dist");
        let index_html = tokio::fs::read_to_string(frontend_dir.join("index.html"))
            .await
            .unwrap();
        let asset_path = index_html
            .split('"')
            .find(|value| value.starts_with("/assets/") && value.ends_with(".js"))
            .unwrap();
        let request = Request::builder()
            .uri(asset_path)
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            response.headers().get(CACHE_CONTROL).unwrap(),
            "public, max-age=31536000, immutable"
        );

        for path in [
            "/assets/../Cargo.toml",
            "/assets/%2E%2E/Cargo.toml",
            "/assets/nested/../../Cargo.toml",
            "/assets/not-found.js",
        ] {
            let request = Request::builder().uri(path).body(Body::empty()).unwrap();
            assert_eq!(
                app.clone().oneshot(request).await.unwrap().status(),
                StatusCode::NOT_FOUND,
                "{path}"
            );
        }
    }
}
