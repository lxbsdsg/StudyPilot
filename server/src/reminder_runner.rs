use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use chrono::{Local, NaiveDateTime};
use rusqlite::Connection;
use serde_json::Value;
use tracing::{error, warn};

use crate::reminders::parse_reminder_time;
use crate::storage::{Store, StoreInner};

pub const DEFAULT_SCAN_INTERVAL: Duration = Duration::from_secs(20);

fn due_reminder_ids(connection: &Connection, now: NaiveDateTime) -> anyhow::Result<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT id, remind_at FROM reminders
         WHERE enabled = 1 ORDER BY remind_at ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut ids = Vec::new();
    for row in rows {
        let (id, remind_at) = row?;
        if parse_reminder_time(&remind_at).is_ok_and(|scheduled| scheduled <= now) {
            ids.push(id);
        }
    }
    Ok(ids)
}

pub fn scan_once_at<F>(database: &Path, now: NaiveDateTime, mut notify: F) -> anyhow::Result<usize>
where
    F: FnMut(&Value) -> anyhow::Result<()>,
{
    let store = StoreInner::open(database)?;
    let connection = Connection::open(database)?;
    let ids = due_reminder_ids(&connection, now)?;
    let mut delivered = 0;
    for id in ids {
        let Some(reminder) = store.claim_due_reminder(&id, now)? else {
            continue;
        };
        delivered += 1;
        if let Err(error) = notify(&reminder) {
            // The claim has already advanced the database, so a failed system
            // notification cannot cause the same reminder to be shown again.
            warn!("提醒已触发，但系统通知发送失败：{error:#}");
        }
    }
    Ok(delivered)
}

pub fn run_reminder_runner<F, Fut>(
    data_dir: PathBuf,
    interval: Duration,
    mut notify: F,
) -> tokio::task::JoinHandle<()>
where
    F: FnMut(Value) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send,
{
    tokio::spawn(async move {
        let database = data_dir.join("studypilot.db");
        let mut scan = tokio::time::interval(interval);
        scan.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            scan.tick().await;
            let store = match StoreInner::open(&database)
                .with_context(|| format!("无法打开提醒数据库 {}", database.display()))
            {
                Ok(store) => store,
                Err(error) => {
                    error!("提醒扫描无法打开数据库，下轮重试：{error:#}");
                    continue;
                }
            };
            let due = match due_reminder_ids_for_store(&store) {
                Ok(due) => due,
                Err(error) => {
                    warn!("提醒扫描失败，下轮重试：{error:#}");
                    continue;
                }
            };
            for id in due {
                let now = Local::now().naive_local();
                let reminder = match store.claim_due_reminder(&id, now) {
                    Ok(Some(reminder)) => reminder,
                    Ok(None) => continue,
                    Err(error) => {
                        warn!("认领提醒 {id} 失败，下轮重试：{error:#}");
                        continue;
                    }
                };
                if let Err(error) = notify(reminder).await {
                    warn!("提醒已触发，但通知发送失败：{error:#}");
                }
            }
        }
    })
}

fn due_reminder_ids_for_store(store: &Store) -> anyhow::Result<Vec<String>> {
    let connection = Connection::open(store.database_path())?;
    due_reminder_ids(&connection, Local::now().naive_local())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M").unwrap()
    }

    #[test]
    fn scan_claims_each_reminder_once_and_continues_after_notification_failure() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("test.db");
        let store = StoreInner::open(&database).unwrap();
        store
            .add_reminder("第一次扫描", "2026-11-16T08:00", None, None, "once")
            .unwrap();
        store
            .add_reminder("第二次扫描", "2026-11-16T08:01", None, None, "daily")
            .unwrap();
        let now = date("2026-11-16 08:02");
        let mut calls = 0;

        let delivered = scan_once_at(&database, now, |reminder| {
            calls += 1;
            assert!(reminder["last_triggered"].as_i64().unwrap() > 0);
            if calls == 1 {
                anyhow::bail!("模拟通知权限不足");
            }
            Ok(())
        })
        .unwrap();

        assert_eq!(delivered, 2);
        assert_eq!(calls, 2);
        assert_eq!(
            scan_once_at(&database, now, |_| Ok(())).unwrap(),
            0,
            "同一次提醒不能被重复认领"
        );
        let reminders = store.list_reminders().unwrap();
        assert!(
            reminders
                .iter()
                .all(|reminder| reminder["last_triggered"] == now.and_utc().timestamp())
        );
        assert_eq!(
            reminders
                .iter()
                .filter(|reminder| reminder["enabled"] == true)
                .count(),
            1
        );
    }
}
