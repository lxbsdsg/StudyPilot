#[cfg(test)]
use crate::adapters::PlatformAdapter;
use crate::{
    adapters::{create_adapter, execution_mode},
    domain::TaskSnapshot,
    storage::Store,
};
use chrono::Local;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct StudyPilotService {
    store: Store,
    refresh_lock: Arc<Mutex<()>>,
}

#[derive(Debug)]
pub struct ServiceError(anyhow::Error);

impl std::fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for ServiceError {}

pub type ServiceResult<T> = Result<T, ServiceError>;

#[cfg(test)]
#[derive(Default)]
pub struct TestAdapter {
    pub result: Option<anyhow::Result<crate::domain::RefreshResult>>,
}

#[cfg(test)]
#[async_trait::async_trait]
impl PlatformAdapter for TestAdapter {
    async fn refresh(&mut self) -> anyhow::Result<crate::domain::RefreshResult> {
        self.result
            .take()
            .unwrap_or_else(|| Err(anyhow::anyhow!("测试适配器没有结果")))
    }

    async fn lookup_essay(&mut self, _essay_id: &str) -> anyhow::Result<TaskSnapshot> {
        Err(anyhow::anyhow!("测试适配器不支持作文查询"))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RefreshSummary {
    pub ok: bool,
    pub tasks: usize,
    pub courses: usize,
    pub warning: String,
}

impl StudyPilotService {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            refresh_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    #[cfg(test)]
    async fn refresh_with_adapter(
        &self,
        account_id: &str,
        adapter: &mut (impl PlatformAdapter + ?Sized),
    ) -> ServiceResult<RefreshSummary> {
        let account = self
            .store
            .get_account(account_id)
            .map_err(ServiceError)?
            .ok_or_else(|| ServiceError(anyhow::anyhow!("账号不存在")))?;
        self.store
            .mark_refreshing(account_id)
            .map_err(ServiceError)?;
        let result = adapter.refresh().await;
        self.apply_refresh_result(account_id, &account.platform, result)
            .await
    }

    pub async fn refresh_account(&self, account_id: &str) -> ServiceResult<RefreshSummary> {
        let refresh_future = self.refresh_account_inner(account_id);
        Box::pin(refresh_future).await
    }

    async fn refresh_account_inner(&self, account_id: &str) -> ServiceResult<RefreshSummary> {
        let (account, password) = self
            .store
            .load_credentials(account_id)
            .map_err(ServiceError)?
            .ok_or_else(|| ServiceError(anyhow::anyhow!("账号不存在")))?;
        if password.is_empty() {
            return Err(ServiceError(anyhow::anyhow!(
                "该账号没有可用凭据，请重新添加账号"
            )));
        }
        self.store
            .mark_refreshing(account_id)
            .map_err(ServiceError)?;
        let platform = account.platform.clone();
        let adapter = create_adapter(&platform, account, password);
        let result = match adapter {
            Ok(mut adapter) => adapter.refresh().await,
            Err(error) => Err(error),
        };
        self.apply_refresh_result(account_id, &platform, result)
            .await
    }

    async fn apply_refresh_result(
        &self,
        account_id: &str,
        platform: &str,
        result: anyhow::Result<crate::domain::RefreshResult>,
    ) -> ServiceResult<RefreshSummary> {
        match result {
            Ok(refreshed) => {
                let _guard = self.refresh_lock.lock().await;
                let replaced = self
                    .store
                    .replace_account_snapshot(
                        account_id,
                        &refreshed.session,
                        refreshed
                            .account
                            .as_ref()
                            .map(|item| item.display_name.as_str()),
                        refreshed.account.as_ref().map(|item| item.user_id.as_str()),
                        refreshed.account.as_ref().map(|item| item.school.as_str()),
                        Local::now().timestamp(),
                        &refreshed.courses,
                        &refreshed.tasks,
                    )
                    .map_err(ServiceError)?;
                if !replaced {
                    return Err(ServiceError(anyhow::anyhow!("账号不存在")));
                }
                self.store
                    .set_platform_check(
                        platform,
                        "verified",
                        match execution_mode(platform) {
                            "python-compatibility" => {
                                "Rust 托管的 Python 兼容适配器已完成解析，真实账号仍需单独验证"
                            }
                            _ => "Rust 适配器已完成离线解析，真实账号仍需单独验证",
                        },
                    )
                    .map_err(ServiceError)?;
                Ok(RefreshSummary {
                    ok: true,
                    tasks: refreshed.tasks.len(),
                    courses: refreshed.courses.len(),
                    warning: refreshed.warning,
                })
            }
            Err(error) => {
                let _guard = self.refresh_lock.lock().await;
                self.store
                    .note_refresh_error(account_id, &error.to_string())
                    .map_err(ServiceError)?;
                Err(ServiceError(error))
            }
        }
    }

    pub async fn lookup_pigai_essay(
        &self,
        account_id: &str,
        essay_id: &str,
    ) -> ServiceResult<TaskSnapshot> {
        let lookup_future = self.lookup_pigai_essay_inner(account_id, essay_id);
        Box::pin(lookup_future).await
    }

    async fn lookup_pigai_essay_inner(
        &self,
        account_id: &str,
        essay_id: &str,
    ) -> ServiceResult<TaskSnapshot> {
        if essay_id.is_empty() || !essay_id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(ServiceError(anyhow::anyhow!("作文号必须是数字")));
        }
        let (account, password) = self
            .store
            .load_credentials(account_id)
            .map_err(ServiceError)?
            .ok_or_else(|| ServiceError(anyhow::anyhow!("账号不存在")))?;
        if account.platform != "pigai" {
            return Err(ServiceError(anyhow::anyhow!(
                "只有批改网账号支持按作文号查询"
            )));
        }
        let mut adapter =
            crate::adapters::PigaiAdapter::new(account, password).map_err(ServiceError)?;
        let task = crate::adapters::PlatformAdapter::lookup_essay(&mut adapter, essay_id)
            .await
            .map_err(ServiceError)?;
        self.store
            .update_account(
                account_id,
                "ready",
                "",
                &adapter.session().to_value(),
                None,
                None,
                None,
                0,
            )
            .map_err(ServiceError)?;
        self.store
            .upsert_task_snapshot(account_id, &task)
            .map_err(ServiceError)?;
        Ok(task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CourseSnapshot, RefreshResult, TaskSnapshot};
    use crate::storage::StoreInner;
    use serde_json::json;

    fn task(platform: &str, title: &str) -> TaskSnapshot {
        TaskSnapshot {
            platform: platform.to_owned(),
            course_id: format!("{platform}:all"),
            course_name: "聚合任务".to_owned(),
            title: title.to_owned(),
            task_id: "1".to_owned(),
            status: "pending".to_owned(),
            deadline: Some("2099-12-31 23:59".to_owned()),
            url: "https://www.pigai.org/".to_owned(),
            detail: "作文".to_owned(),
            source: "wait-complete".to_owned(),
            raw: json!({}),
        }
    }

    fn snapshot(platform: &str, title: &str) -> RefreshResult {
        RefreshResult {
            courses: vec![CourseSnapshot {
                platform: platform.to_owned(),
                id: format!("{platform}:all"),
                name: "聚合课程".to_owned(),
                progress: 90,
                raw: json!({"pending": 1}),
            }],
            tasks: vec![task(platform, title)],
            account: None,
            warning: String::new(),
            session: json!({
                "version": 1,
                "cookies": [{"name": "auth", "value": "new"}],
                "uses_chromium_login": true
            }),
        }
    }

    #[tokio::test]
    async fn successful_refresh_commits_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let account = store
            .add_legacy_account("pigai", "tester", "password", "")
            .unwrap();
        let service = StudyPilotService::new(store.clone());
        let mut adapter = TestAdapter {
            result: Some(Ok(snapshot("pigai", "New task"))),
        };

        let summary = service
            .refresh_with_adapter(&account.id, &mut adapter)
            .await
            .unwrap();

        assert_eq!(summary.tasks, 1);
        assert_eq!(
            store.list_tasks(Some(&account.id), None, None).unwrap()[0]["title"],
            "New task"
        );
        assert_eq!(
            store.account_session(&account.id).unwrap().unwrap()["cookies"][0]["value"],
            "new"
        );
        let account = store.get_account(&account.id).unwrap().unwrap();
        assert_eq!(account.status, "ready");
        assert!(account.last_refresh > 0);
        assert!(
            store
                .get_platform_checks()
                .unwrap()
                .get("pigai")
                .is_some_and(|check| check["note"]
                    .as_str()
                    .is_some_and(|note| note.starts_with("Rust 适配器")))
        );
    }

    #[tokio::test]
    async fn compatibility_refresh_reports_python_mode() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let account = store
            .add_legacy_account("welearn", "tester", "password", "")
            .unwrap();
        let service = StudyPilotService::new(store.clone());
        let mut adapter = TestAdapter {
            result: Some(Ok(snapshot("welearn", "Compatibility task"))),
        };

        service
            .refresh_with_adapter(&account.id, &mut adapter)
            .await
            .unwrap();

        let check = store.get_platform_checks().unwrap();
        assert!(
            check["welearn"]["note"]
                .as_str()
                .unwrap()
                .contains("Python 兼容适配器")
        );
    }

    #[tokio::test]
    async fn failed_refresh_keeps_cache_and_session() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let account = store
            .add_legacy_account("pigai", "tester", "password", "")
            .unwrap();
        let old_session = json!({"version": 1, "cookies": [{"name": "auth", "value": "old"}]});
        store
            .replace_account_snapshot(
                &account.id,
                &old_session,
                None,
                None,
                None,
                42,
                &[],
                &[task("pigai", "Cached task")],
            )
            .unwrap();
        let before = store.session_bytes(&account.id).unwrap();
        let service = StudyPilotService::new(store.clone());
        let mut adapter = TestAdapter {
            result: Some(Err(anyhow::anyhow!("登录态已失效"))),
        };

        assert!(
            service
                .refresh_with_adapter(&account.id, &mut adapter)
                .await
                .is_err()
        );

        let stored = store.get_account(&account.id).unwrap().unwrap();
        assert_eq!(stored.status, "auth_error");
        assert_eq!(stored.last_error, "登录态已失效");
        assert_eq!(store.session_bytes(&account.id).unwrap(), before);
        assert_eq!(
            store.list_tasks(Some(&account.id), None, None).unwrap()[0]["title"],
            "Cached task"
        );
        assert_eq!(store.list_courses(Some(&account.id)).unwrap().len(), 0);
    }
}
