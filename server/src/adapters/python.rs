use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    config::repository_root,
    domain::{Account, RefreshResult, TaskSnapshot},
};

const PROTOCOL_VERSION: u8 = 1;
const BRIDGE_ENTRYPOINT: &str = "studypilot.compat_bridge";
const TIMEOUT: Duration = Duration::from_secs(180);
const MAX_STDOUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct PythonAdapter {
    account: Account,
    password: String,
    runtime: PythonRuntime,
}

#[derive(Debug, Clone)]
struct PythonRuntime {
    executable: Arc<PathBuf>,
    working_directory: Arc<PathBuf>,
}

#[derive(Debug, Serialize)]
struct BridgeRequest<'a> {
    protocol_version: u8,
    platform: &'a str,
    username: &'a str,
    password: &'a str,
    session: &'a Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeResponse {
    ok: bool,
    protocol_version: u8,
    #[serde(default)]
    result: Option<RefreshResult>,
    #[serde(default)]
    error: Option<String>,
}

impl PythonAdapter {
    pub fn new(account: Account, password: String) -> anyhow::Result<Self> {
        if !matches!(account.platform.as_str(), "unipus" | "welearn" | "chaoxing") {
            bail!("平台不允许通过 Python 兼容桥接刷新");
        }
        Ok(Self {
            account,
            password,
            runtime: locate_runtime()?,
        })
    }

    fn decode_response(stdout: &[u8], stderr: &[u8]) -> anyhow::Result<BridgeResponse> {
        let response = match serde_json::from_slice::<BridgeResponse>(stdout) {
            Ok(response) => response,
            Err(error) => {
                let diagnostic = String::from_utf8_lossy(stderr);
                let diagnostic = diagnostic
                    .lines()
                    .next_back()
                    .unwrap_or("无诊断输出")
                    .trim();
                bail!("Python 兼容适配器未返回有效 JSON：{error}；{diagnostic}");
            }
        };
        if response.protocol_version != PROTOCOL_VERSION {
            bail!("Python 兼容适配器协议版本不匹配");
        }
        if !response.ok {
            bail!(
                "{}",
                response
                    .error
                    .filter(|message| !message.trim().is_empty())
                    .unwrap_or_else(|| "Python 兼容适配器刷新失败".to_owned())
            );
        }
        Ok(response)
    }

    fn payload(&self) -> anyhow::Result<Vec<u8>> {
        serde_json::to_vec(&BridgeRequest {
            protocol_version: PROTOCOL_VERSION,
            platform: &self.account.platform,
            username: &self.account.username,
            password: &self.password,
            session: &self.account.session,
        })
        .context("无法编码 Python 兼容请求")
    }

    async fn run_bridge(&self) -> anyhow::Result<RefreshResult> {
        let payload = self.payload()?;
        let executable = self.runtime.executable.clone();
        let working_directory = self.runtime.working_directory.clone();
        tokio::spawn(
            async move { run_bridge_process(&executable, &working_directory, &payload).await },
        )
        .await
        .map_err(|error| anyhow::anyhow!("Python 兼容任务异常退出：{error}"))?
    }
}

#[async_trait]
impl super::PlatformAdapter for PythonAdapter {
    async fn refresh(&mut self) -> anyhow::Result<RefreshResult> {
        let result = self.run_bridge().await?;
        validate_refresh_result(&self.account.platform, &result)?;
        Ok(result)
    }

    async fn lookup_essay(&mut self, _essay_id: &str) -> anyhow::Result<TaskSnapshot> {
        bail!("只有批改网账号支持按作文号查询")
    }
}

fn validate_refresh_result(platform: &str, result: &RefreshResult) -> anyhow::Result<()> {
    for course in &result.courses {
        anyhow::ensure!(
            course.platform == platform,
            "Python 兼容适配器返回了其他平台的数据"
        );
        anyhow::ensure!(!course.id.is_empty(), "Python 兼容适配器返回了空课程 ID");
    }
    for task in &result.tasks {
        anyhow::ensure!(
            task.platform == platform,
            "Python 兼容适配器返回了其他平台的数据"
        );
        anyhow::ensure!(
            !task.course_id.is_empty() && !task.title.is_empty(),
            "Python 兼容适配器返回了不完整任务"
        );
        anyhow::ensure!(
            matches!(task.status.as_str(), "pending" | "completed" | "ignored"),
            "Python 兼容适配器返回了无效任务状态"
        );
    }
    if let Some(account) = &result.account {
        anyhow::ensure!(
            account.platform == platform,
            "Python 兼容适配器返回了其他平台的账号信息"
        );
    }
    Ok(())
}

fn locate_runtime() -> anyhow::Result<PythonRuntime> {
    let root = if let Some(value) = std::env::var_os("STUDYPILOT_PYTHON_ROOT") {
        let path = PathBuf::from(value);
        anyhow::ensure!(
            path.join("studypilot/adapters/__init__.py").is_file(),
            "STUDYPILOT_PYTHON_ROOT 中缺少 Python 兼容适配器"
        );
        path
    } else {
        repository_root().ok_or_else(|| {
            anyhow::anyhow!(
                "未找到 Python 兼容适配器；请设置 STUDYPILOT_PYTHON_ROOT，或安装包含 Python 运行时的桌面版本"
            )
        })?
    };
    let executable = runtime_executable(&root)?;
    Ok(PythonRuntime {
        executable: Arc::new(executable),
        working_directory: Arc::new(root),
    })
}

fn runtime_executable(root: &Path) -> anyhow::Result<PathBuf> {
    let candidates = match std::env::var_os("STUDYPILOT_PYTHON") {
        Some(value) => vec![PathBuf::from(value)],
        None => {
            let base = root.parent().unwrap_or(root).to_owned();
            let mut candidates = vec![
                base.join(".venv/Scripts/python.exe"),
                root.join(".venv/Scripts/python.exe"),
                base.join(".venv/bin/python"),
                root.join(".venv/bin/python"),
            ];
            if cfg!(windows) {
                candidates.push(PathBuf::from("pythonw.exe"));
                candidates.push(PathBuf::from("python.exe"));
            } else {
                candidates.push(PathBuf::from("python3"));
            }
            candidates
        }
    };
    for path in candidates {
        if !path.is_absolute() || !path.is_file() {
            if matches!(
                path.to_str(),
                Some("pythonw.exe" | "python.exe" | "python3")
            ) {
                return Ok(path);
            }
        }
        if path.is_file() {
            return Ok(path);
        }
    }
    anyhow::bail!("未找到 Python 解释器；请设置 STUDYPILOT_PYTHON")
}

async fn run_bridge_process(
    executable: &Path,
    root: &Path,
    payload: &[u8],
) -> anyhow::Result<RefreshResult> {
    let mut command = tokio::process::Command::new(executable);
    command
        .arg("-m")
        .arg(BRIDGE_ENTRYPOINT)
        .current_dir(root)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command.spawn().with_context(|| {
        format!(
            "无法启动 Python 兼容适配器 {}（Python 运行时目录 {}）",
            executable.display(),
            root.display()
        )
    })?;
    let mut stdin = child.stdin.take().context("无法连接 Python 兼容输入")?;
    let mut stdout = child.stdout.take().context("无法连接 Python 兼容输出")?;
    let mut stderr = child.stderr.take().context("无法连接 Python 兼容诊断")?;
    let output = async {
        stdin.write_all(payload).await?;
        stdin.shutdown().await?;
        drop(stdin);
        let (stdout, stderr) = tokio::join!(
            async { read_limited(&mut stdout, MAX_STDOUT_BYTES, "输出").await },
            async { read_limited(&mut stderr, MAX_STDERR_BYTES, "诊断").await }
        );
        let (stdout, stderr) = (stdout?, stderr?);
        let status = child.wait().await?;
        Ok::<_, anyhow::Error>((status, stdout, stderr))
    };
    let (status, stdout, stderr) = tokio::time::timeout(TIMEOUT, output)
        .await
        .map_err(|_| anyhow::anyhow!("Python 兼容适配器超时"))??;
    if !status.success() {
        // Preserve structured adapter errors while ensuring neither stdout nor
        // diagnostics can leak the credential sent over stdin.
        if let Ok(response) = serde_json::from_slice::<BridgeResponse>(&stdout) {
            if response.protocol_version == PROTOCOL_VERSION && !response.ok {
                bail!(
                    "{}",
                    response
                        .error
                        .filter(|message| !message.trim().is_empty())
                        .unwrap_or_else(|| "Python 兼容适配器刷新失败".to_owned())
                );
            }
        }
        let diagnostic = String::from_utf8_lossy(&stderr);
        let diagnostic = diagnostic
            .lines()
            .next_back()
            .unwrap_or("无诊断输出")
            .trim();
        bail!("Python 兼容适配器退出状态异常：{diagnostic}");
    }
    let response = PythonAdapter::decode_response(&stdout, &stderr)?;
    response
        .result
        .ok_or_else(|| anyhow::anyhow!("Python 兼容适配器未返回刷新结果"))
}

async fn read_limited(
    reader: &mut (impl AsyncReadExt + Unpin),
    limit: usize,
    label: &str,
) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await?;
    anyhow::ensure!(
        bytes.len() <= limit,
        "Python 兼容适配器{label}超过 {limit} 字节上限"
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AccountInfo, CourseSnapshot, RefreshResult, TaskSnapshot};
    use serde_json::json;
    use tokio::io::duplex;

    fn account(platform: &str) -> Account {
        Account {
            platform: platform.to_owned(),
            username: "student".to_owned(),
            session: json!({"version": 1, "cookies": []}),
            ..Account::default()
        }
    }

    #[test]
    fn adapter_rejects_platforms_outside_bridge_allowlist() {
        assert!(PythonAdapter::new(account("pigai"), "secret".to_owned()).is_err());
    }

    #[test]
    fn bridge_request_keeps_password_out_of_argv_shape() {
        let runtime = PythonRuntime {
            executable: Arc::new(PathBuf::from("python.exe")),
            working_directory: Arc::new(PathBuf::from(".")),
        };
        let adapter = PythonAdapter {
            account: account("welearn"),
            password: "not-in-argv".to_owned(),
            runtime,
        };
        let encoded = String::from_utf8(adapter.payload().unwrap()).unwrap();
        assert!(encoded.contains("not-in-argv"));
        assert!(!encoded.contains("database_path"));
        assert!(!encoded.contains("secret_path"));
    }

    #[test]
    fn response_errors_are_parsed_without_malformed_output() {
        let failed = serde_json::to_vec(&serde_json::json!({
            "ok": false,
            "protocol_version": 1,
            "error": "登录态已失效",
        }))
        .unwrap();
        assert!(PythonAdapter::decode_response(&failed, b"").is_err());
        assert!(PythonAdapter::decode_response(b"not valid json", b"Python failed",).is_err());
    }

    #[test]
    fn validates_snapshot_platform_and_task_shape() {
        let result = RefreshResult {
            courses: vec![CourseSnapshot {
                platform: "welearn".to_owned(),
                id: "course".to_owned(),
                name: "课程".to_owned(),
                progress: 0,
                raw: Value::Null,
            }],
            tasks: vec![TaskSnapshot {
                platform: "welearn".to_owned(),
                course_id: "course".to_owned(),
                title: "任务".to_owned(),
                status: "pending".to_owned(),
                ..TaskSnapshot::default()
            }],
            account: Some(AccountInfo {
                platform: "welearn".to_owned(),
                username: "student".to_owned(),
                ..AccountInfo::default()
            }),
            ..RefreshResult::default()
        };
        assert!(validate_refresh_result("welearn", &result).is_ok());
        let mut invalid = result;
        invalid.tasks[0].platform = "chaoxing".to_owned();
        assert!(validate_refresh_result("welearn", &invalid).is_err());
    }

    #[test]
    fn launches_python_bridge_with_stdin_and_structured_output() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned();
        let executable = root.join(".venv/Scripts/python.exe");
        assert!(
            executable.is_file(),
            "repository Python virtualenv is available"
        );
        let payload = serde_json::to_vec(&serde_json::json!({
            "protocol_version": 1,
            "platform": "pigai",
            "username": "student",
            "password": "not-in-argv",
            "session": {},
        }))
        .unwrap();
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(run_bridge_process(&executable, &root, &payload));
        let error = result.unwrap_err().to_string();
        assert!(error.contains("平台不允许"), "{error}");
        assert!(!error.contains("not-in-argv"), "{error}");
    }

    #[tokio::test]
    async fn limits_bridge_process_output() {
        for (label, limit) in [("输出", 4), ("诊断", 3)] {
            let (mut writer, mut reader) = duplex(64);
            let write = tokio::spawn(async move {
                async {
                    writer.write_all(b"oversized").await?;
                    writer.shutdown().await
                }
                .await
                .unwrap();
            });
            let error = read_limited(&mut reader, limit, label).await.unwrap_err();
            write.await.unwrap();
            let message = error.to_string();
            assert!(
                message.contains(&format!("{label}超过")),
                "unexpected error: {message}"
            );
            assert!(!message.contains("oversized"), "{message}");
        }
    }
}
