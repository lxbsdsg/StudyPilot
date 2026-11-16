mod pigai;
mod python;

pub use pigai::{PigaiAdapter, PigaiSession};

use crate::domain::{Account, RefreshResult, TaskSnapshot};
use async_trait::async_trait;

pub trait BrowserCredential {
    fn password(&self) -> &str;
}

impl BrowserCredential for (Account, String) {
    fn password(&self) -> &str {
        &self.1
    }
}

#[async_trait]
pub trait PlatformAdapter {
    async fn refresh(&mut self) -> anyhow::Result<RefreshResult>;
    async fn lookup_essay(&mut self, essay_id: &str) -> anyhow::Result<TaskSnapshot>;
}

pub fn create_adapter(
    platform: &str,
    account: Account,
    password: String,
) -> anyhow::Result<Box<dyn PlatformAdapter + Send>> {
    match platform {
        "pigai" => Ok(Box::new(PigaiAdapter::new(account, password)?)),
        "unipus" | "welearn" | "chaoxing" => {
            Ok(Box::new(python::PythonAdapter::new(account, password)?))
        }
        other => anyhow::bail!("不支持的平台：{other}"),
    }
}

pub fn platform_label(platform: &str) -> &str {
    match platform {
        "unipus" => "U校园",
        "welearn" => "WELearn",
        "chaoxing" => "学习通",
        "pigai" => "批改网",
        other => other,
    }
}

pub fn execution_mode(platform: &str) -> &str {
    match platform {
        "pigai" => "native-rust",
        "unipus" | "welearn" | "chaoxing" => "python-compatibility",
        _ => "unsupported",
    }
}
