use chrono::{Datelike, Duration, NaiveDateTime, Weekday};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

pub fn parse_reminder_time(value: &str) -> anyhow::Result<NaiveDateTime> {
    let value = value.trim();
    for format in ["%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(value) = NaiveDateTime::parse_from_str(value, format) {
            return Ok(value);
        }
    }
    anyhow::bail!("提醒时间格式无效")
}

pub fn next_recurring_time(
    scheduled: NaiveDateTime,
    repeat_rule: &str,
    now: NaiveDateTime,
) -> anyhow::Result<NaiveDateTime> {
    let mut next = scheduled;
    match repeat_rule {
        "daily" => {
            while next <= now {
                next += Duration::days(1);
            }
        }
        "weekly" => {
            while next <= now {
                next += Duration::weeks(1);
            }
        }
        "weekdays" => {
            if next.weekday() == Weekday::Sat {
                next += Duration::days(2);
            } else if next.weekday() == Weekday::Sun {
                next += Duration::days(1);
            }
            while next <= now {
                next += match next.weekday() {
                    Weekday::Fri => Duration::days(3),
                    _ => Duration::days(1),
                };
            }
        }
        "once" => anyhow::bail!("一次性提醒没有下一次触发时间"),
        _ => anyhow::bail!("无效的重复规则"),
    }
    Ok(next)
}

pub fn claim_due_reminder(
    connection: &mut Connection,
    reminder_id: &str,
    now: NaiveDateTime,
) -> anyhow::Result<Option<Value>> {
    let transaction = connection.transaction()?;
    let row = transaction
        .query_row(
            "SELECT remind_at, repeat_rule, enabled FROM reminders WHERE id = ?",
            [reminder_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                ))
            },
        )
        .optional()?;
    let Some((remind_at, repeat_rule, enabled)) = row else {
        return Ok(None);
    };
    if !enabled {
        return Ok(None);
    }
    let scheduled = parse_reminder_time(&remind_at)?;
    if scheduled > now {
        return Ok(None);
    }

    let timestamp = now.and_utc().timestamp();
    let next = if repeat_rule == "once" {
        None
    } else {
        Some(
            next_recurring_time(scheduled, &repeat_rule, now)?
                .format("%Y-%m-%dT%H:%M")
                .to_string(),
        )
    };
    let claimed = transaction.execute(
        "UPDATE reminders
         SET last_triggered = ?, enabled = ?,
             remind_at = COALESCE(?, remind_at)
         WHERE id = ? AND enabled = 1 AND remind_at = ?",
        params![
            timestamp,
            i64::from(next.is_some()),
            next,
            reminder_id,
            remind_at
        ],
    )?;
    if claimed == 0 {
        return Ok(None);
    }
    let reminder = transaction.query_row(
        "SELECT r.*, t.title AS task_title, a.platform, a.label AS account_label
         FROM reminders r
         LEFT JOIN tasks t ON t.id = r.task_id
         LEFT JOIN accounts a ON a.id = r.account_id
         WHERE r.id = ?",
        [reminder_id],
        |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>("id")?,
                "account_id": row.get::<_, Option<String>>("account_id")?,
                "task_id": row.get::<_, Option<String>>("task_id")?,
                "title": row.get::<_, String>("title")?,
                "remind_at": row.get::<_, String>("remind_at")?,
                "repeat_rule": row.get::<_, String>("repeat_rule")?,
                "enabled": row.get::<_, i64>("enabled")? != 0,
                "last_triggered": timestamp,
                "created_at": row.get::<_, i64>("created_at")?,
                "task_title": row.get::<_, Option<String>>("task_title")?,
                "platform": row.get::<_, Option<String>>("platform")?,
                "account_label": row.get::<_, Option<String>>("account_label")?,
            }))
        },
    )?;
    transaction.commit()?;
    Ok(Some(reminder))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StoreInner;

    fn date(value: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M").unwrap()
    }

    #[test]
    fn recurring_rules_skip_periods_already_elapsed() {
        assert_eq!(
            next_recurring_time(date("2026-11-16 08:00"), "daily", date("2026-11-18 08:05"))
                .unwrap(),
            date("2026-11-19 08:00")
        );
        assert_eq!(
            next_recurring_time(date("2026-11-16 08:00"), "weekly", date("2026-11-18 08:05"))
                .unwrap(),
            date("2026-11-23 08:00")
        );
    }

    #[test]
    fn weekday_rule_moves_friday_and_weekends_to_workdays() {
        assert_eq!(
            next_recurring_time(
                date("2026-11-20 08:00"),
                "weekdays",
                date("2026-11-20 08:01")
            )
            .unwrap(),
            date("2026-11-23 08:00")
        );
        assert_eq!(
            next_recurring_time(
                date("2026-11-21 08:00"),
                "weekdays",
                date("2026-11-19 08:00")
            )
            .unwrap(),
            date("2026-11-23 08:00")
        );
        assert_eq!(
            next_recurring_time(
                date("2026-11-20 08:00"),
                "weekdays",
                date("2026-11-21 09:00")
            )
            .unwrap(),
            date("2026-11-23 08:00")
        );
    }

    #[test]
    fn delivered_once_is_disabled_and_cannot_be_claimed_twice() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let reminder = store
            .add_reminder("到期提醒", "2026-11-16T08:00", None, None, "once")
            .unwrap();
        let id = reminder["id"].as_str().unwrap();
        let mut connection = store.connect_for_test().unwrap();
        let claimed = claim_due_reminder(&mut connection, id, date("2026-11-16 08:01"))
            .unwrap()
            .expect("提醒应该到期");
        assert_eq!(claimed["title"], "到期提醒");
        assert_eq!(store.get_reminder(id).unwrap().unwrap()["enabled"], false);
        assert!(
            claim_due_reminder(&mut connection, id, date("2026-11-16 08:02"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn delivered_recurring_reminder_advances_instead_of_disabling() {
        let directory = tempfile::tempdir().unwrap();
        let store = StoreInner::open(directory.path().join("test.db")).unwrap();
        let reminder = store
            .add_reminder("每日提醒", "2026-11-16T08:00", None, None, "daily")
            .unwrap();
        let id = reminder["id"].as_str().unwrap();
        let mut connection = store.connect_for_test().unwrap();
        claim_due_reminder(&mut connection, id, date("2026-11-16 08:01")).unwrap();
        let current = store.get_reminder(id).unwrap().unwrap();
        assert_eq!(current["enabled"], true);
        assert_eq!(current["remind_at"], "2026-11-17T08:00");
        assert!(current["last_triggered"].as_i64().unwrap() > 0);
    }
}
