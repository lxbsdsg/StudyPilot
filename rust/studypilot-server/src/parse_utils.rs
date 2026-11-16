use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime};
use regex::Regex;
use std::sync::LazyLock;

static FULL_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(20\d{2})[-/.年](\d{1,2})[-/.月](\d{1,2})(?:[日号]?\s*(\d{1,2})(?:[:：](\d{2}))?)?",
    )
    .expect("valid date regex")
});

static SHORT_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d{1,2})[-/.月](\d{1,2}).*?(\d{1,2})[:：](\d{2})?")
        .expect("valid short date regex")
});

fn parse_number(value: Option<&str>) -> Option<u32> {
    value?.parse().ok()
}

fn format_datetime(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M").to_string()
}

fn parse_short_date(raw: &str, now: NaiveDateTime) -> Option<NaiveDateTime> {
    let captures = SHORT_DATE.captures(raw)?;
    let month = parse_number(captures.get(1).map(|m| m.as_str()))?;
    let day = parse_number(captures.get(2).map(|m| m.as_str()))?;
    let hour = parse_number(captures.get(3).map(|m| m.as_str()))?;
    let minute = parse_number(captures.get(4).map(|m| m.as_str())).unwrap_or(0);
    let mut value =
        NaiveDate::from_ymd_opt(now.year(), month, day)?.and_hms_opt(hour, minute, 0)?;
    if value < now - Duration::days(180) {
        value = value.with_year(value.year() + 1)?;
    }
    Some(value)
}

pub fn parse_deadline_at(text: &str, now: NaiveDateTime) -> Option<String> {
    let raw = text.trim().replace("截止时间", "").replace("截止", "");
    let raw = raw.trim_matches(|c: char| c.is_whitespace() || matches!(c, '：' | ':'));
    if raw.is_empty() {
        return None;
    }
    if let Some(captures) = FULL_DATE.captures(raw) {
        let year = parse_number(captures.get(1).map(|m| m.as_str()))?;
        let month = parse_number(captures.get(2).map(|m| m.as_str()))?;
        let day = parse_number(captures.get(3).map(|m| m.as_str()))?;
        let hour = parse_number(captures.get(4).map(|m| m.as_str())).unwrap_or(0);
        let minute = parse_number(captures.get(5).map(|m| m.as_str())).unwrap_or(0);
        let value =
            NaiveDate::from_ymd_opt(year as i32, month, day)?.and_hms_opt(hour, minute, 0)?;
        return Some(format_datetime(value));
    }
    parse_short_date(raw, now).map(format_datetime)
}

pub fn parse_deadline(text: &str) -> Option<String> {
    parse_deadline_at(text, Local::now().naive_local())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn parses_full_and_chinese_dates() {
        assert_eq!(
            parse_deadline("2026-11-11 23:59").as_deref(),
            Some("2026-11-11 23:59")
        );
        assert_eq!(
            parse_deadline("截止时间：2026年11月11日 23:59").as_deref(),
            Some("2026-11-11 23:59")
        );
        assert_eq!(
            parse_deadline("2026/12/01 18").as_deref(),
            Some("2026-12-01 18:00")
        );
    }

    #[test]
    fn short_date_rolls_into_next_year_after_six_months() {
        let now = NaiveDate::from_ymd_opt(2026, 11, 15)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        assert_eq!(
            parse_deadline_at("1/10 23:59", now).as_deref(),
            Some("2027-01-10 23:59")
        );
    }

    #[test]
    fn rejects_invalid_dates() {
        assert_eq!(parse_deadline("未截止"), None);
        assert_eq!(parse_deadline("2026-02-30 10:00"), None);
    }
}
