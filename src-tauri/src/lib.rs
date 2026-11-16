use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::Value;
use studypilot_core::{
    api,
    backend::{
        AccountInput, ReminderInput, ReminderPatch, StudyPilotBackend, TaskReminderInput, Workspace,
    },
    config::data_dir,
    domain::Account,
    open_targets::TaskOpenTarget,
    reminder_runner::{DEFAULT_SCAN_INTERVAL, run_reminder_runner},
};
use tauri::{
    Emitter, Manager, State,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_notification::NotificationExt;

type CommandResult<T> = Result<T, String>;

struct DesktopState {
    backend: StudyPilotBackend,
    refreshes: Arc<Mutex<HashMap<String, bool>>>,
}

impl DesktopState {
    fn try_begin_refresh(&self, key: &str) -> bool {
        let Ok(mut refreshes) = self.refreshes.lock() else {
            return false;
        };
        if refreshes.contains_key(key) {
            return false;
        }
        refreshes.insert(key.to_owned(), true);
        true
    }

    fn finish_refresh(&self, key: &str) {
        if let Ok(mut refreshes) = self.refreshes.lock() {
            refreshes.remove(key);
        }
    }
}

#[tauri::command]
fn fetch_workspace(state: State<'_, DesktopState>) -> CommandResult<Workspace> {
    state.backend.workspace().map_err(command_error)
}

#[tauri::command]
fn add_account(state: State<'_, DesktopState>, payload: AccountInput) -> CommandResult<Account> {
    state.backend.add_account(&payload).map_err(command_error)
}

#[tauri::command]
fn delete_account(state: State<'_, DesktopState>, account_id: String) -> CommandResult<()> {
    state
        .backend
        .delete_account(&account_id)
        .map_err(command_error)
}

#[tauri::command]
async fn refresh_account(
    state: State<'_, DesktopState>,
    account_id: String,
) -> CommandResult<Value> {
    let key = format!("account:{account_id}");
    if !state.try_begin_refresh(&key) {
        return Err("该账号正在同步，请稍后重试".to_owned());
    }
    let result = match state.backend.refresh_account(&account_id).await {
        Ok(value) => serde_json::to_value(value).map_err(command_error),
        Err(error) => Err(error.to_string()),
    };
    state.finish_refresh(&key);
    result
}

#[tauri::command]
async fn refresh_all(state: State<'_, DesktopState>) -> CommandResult<Value> {
    const KEY: &str = "all";
    if !state.try_begin_refresh(KEY) {
        return Err("全部账号正在同步，请稍后重试".to_owned());
    }
    let result = match state.backend.refresh_all().await {
        Ok(value) => serde_json::to_value(value).map_err(command_error),
        Err(error) => Err(error.to_string()),
    };
    state.finish_refresh(KEY);
    result
}

#[tauri::command]
fn update_task_status(
    state: State<'_, DesktopState>,
    task_id: String,
    status: String,
) -> CommandResult<Value> {
    state
        .backend
        .update_task_status(&task_id, &status)
        .map_err(command_error)?;
    Ok(serde_json::json!({"ok": true}))
}

#[tauri::command]
fn restore_task(state: State<'_, DesktopState>, task_id: String) -> CommandResult<Value> {
    state
        .backend
        .restore_task(&task_id)
        .map_err(command_error)?;
    Ok(serde_json::json!({"ok": true}))
}

#[tauri::command]
fn task_open_target(
    state: State<'_, DesktopState>,
    account_id: String,
    task_id: String,
    mobile: bool,
) -> CommandResult<TaskOpenTarget> {
    state
        .backend
        .task_open_target(&account_id, &task_id, mobile)
        .map_err(command_error)
}

#[tauri::command]
async fn lookup_pigai_essay(
    state: State<'_, DesktopState>,
    account_id: String,
    essay_id: String,
) -> CommandResult<Value> {
    state
        .backend
        .lookup_pigai_essay(&account_id, &essay_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
fn create_task_reminder(
    state: State<'_, DesktopState>,
    task_id: String,
    payload: TaskReminderInput,
) -> CommandResult<Value> {
    state
        .backend
        .create_task_reminder(&task_id, &payload)
        .map_err(command_error)
}

#[tauri::command]
fn create_reminder(state: State<'_, DesktopState>, payload: ReminderInput) -> CommandResult<Value> {
    state
        .backend
        .create_reminder(&payload)
        .map_err(command_error)
}

#[tauri::command]
fn update_reminder(
    state: State<'_, DesktopState>,
    reminder_id: String,
    payload: ReminderPatch,
) -> CommandResult<Value> {
    state
        .backend
        .update_reminder(&reminder_id, &payload)
        .map_err(command_error)
}

#[tauri::command]
fn delete_reminder(state: State<'_, DesktopState>, reminder_id: String) -> CommandResult<()> {
    state
        .backend
        .delete_reminder(&reminder_id)
        .map_err(command_error)
}

fn command_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn delayed_reminder_notification(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                tracing::warn!("无法创建提醒后台运行时：{error:#}");
                return;
            }
        };
        runtime.spawn(async move {
            if let Err(error) = notify_reminders(app).await {
                tracing::warn!("后台提醒任务退出：{error:#}");
            }
        });
        runtime.block_on(async {
            tokio::time::sleep(Duration::MAX).await;
        });
    });
}

async fn notify_reminders(app: tauri::AppHandle) -> anyhow::Result<()> {
    let directory = data_dir()?;
    let runner = run_reminder_runner(directory, DEFAULT_SCAN_INTERVAL, {
        let app = app.clone();
        move |reminder| {
            let app = app.clone();
            async move {
                let title = reminder["title"]
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("StudyPilot 提醒")
                    .to_owned();
                app.emit("reminder.triggered", reminder.clone())?;
                app.notification()
                    .builder()
                    .title("StudyPilot 提醒")
                    .body(title)
                    .show()?;
                Ok(())
            }
        }
    });
    runner.await?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "studypilot_desktop=info".into()),
        )
        .try_init()
        .ok();
    let store = api::open_default_store()?;
    let state = DesktopState {
        backend: StudyPilotBackend::new(store),
        refreshes: Arc::new(Mutex::new(HashMap::new())),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "显示 StudyPilot", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .cloned()
                        .expect("application icon"),
                )
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            delayed_reminder_notification(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            fetch_workspace,
            add_account,
            delete_account,
            refresh_account,
            refresh_all,
            update_task_status,
            restore_task,
            task_open_target,
            lookup_pigai_essay,
            create_task_reminder,
            create_reminder,
            update_reminder,
            delete_reminder,
        ])
        .run(tauri::generate_context!())
        .map_err(Into::into)
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
