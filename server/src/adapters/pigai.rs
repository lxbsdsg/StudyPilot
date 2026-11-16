use crate::domain::{Account, AccountInfo, CourseSnapshot, RefreshResult, TaskSnapshot};
use crate::parse_utils;
use async_trait::async_trait;
use chrono::{Local, NaiveDateTime};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

pub const BASE: &str = "https://www.pigai.org";
pub const WAIT_COMPLETE: &str = "https://www.pigai.org/index.php?c=write&a=waitComplete&bf=4";
pub const OWN_WORK: &str = "https://www.pigai.org/index.php?c=write";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                         (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct PigaiCookie {
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default = "root_path")]
    pub path: String,
}

fn root_path() -> String {
    "/".to_owned()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PigaiSession {
    #[serde(default = "session_version")]
    pub version: u32,
    #[serde(default)]
    pub cookies: Vec<PigaiCookie>,
    #[serde(default)]
    pub uses_chromium_login: bool,
}

fn session_version() -> u32 {
    1
}

impl PigaiSession {
    pub fn from_value(value: &Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_else(|_| Self::empty())
    }

    pub fn empty() -> Self {
        Self {
            version: 1,
            cookies: Vec::new(),
            uses_chromium_login: false,
        }
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({"version": 1, "cookies": []}))
    }

    pub fn cookie_header(&self) -> String {
        self.cookies
            .iter()
            .filter(|cookie| cookie.domain.contains("pigai.org") && !cookie.name.is_empty())
            .map(|cookie| format!("{}={}", cookie.name, cookie.value))
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn has_login_cookie(&self) -> bool {
        self.cookies
            .iter()
            .any(|cookie| cookie.name == "_JUKU_USER" && cookie.value != "")
    }

    pub fn profile(&self) -> Value {
        self.cookies
            .iter()
            .find(|cookie| cookie.name == "_JUKU_USER")
            .and_then(|cookie| {
                url::form_urlencoded::parse(cookie.value.as_bytes())
                    .collect::<Vec<_>>()
                    .iter()
                    .find(|(key, _)| key.is_empty())
                    .map(|(_, value)| value.to_string())
            })
            .and_then(|decoded| serde_json::from_str(&decoded).ok())
            .unwrap_or_else(|| json!({}))
    }
}

pub struct PigaiAdapter {
    account: Account,
    password: String,
    session: PigaiSession,
    client: reqwest::Client,
}

impl PigaiAdapter {
    pub fn new(account: Account, password: String) -> anyhow::Result<Self> {
        let session = PigaiSession::from_value(&account.session);
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .cookie_store(true)
            .build()?;
        Ok(Self {
            account,
            password,
            session,
            client,
        })
    }

    pub fn session(&self) -> &PigaiSession {
        &self.session
    }

    fn profile(&self) -> Value {
        self.session.profile()
    }
}

#[async_trait]
impl super::PlatformAdapter for PigaiAdapter {
    async fn refresh(&mut self) -> anyhow::Result<RefreshResult> {
        if !self.session.has_login_cookie() {
            self.login().await?;
        }
        let header = self.session.cookie_header();
        let mut request = self.client.get(WAIT_COMPLETE);
        if !header.is_empty() {
            request = request.header("Cookie", &header);
        }
        let response = request.send().await?;
        anyhow::ensure!(
            response.status().is_success(),
            "批改网待完成列表请求失败：HTTP {}",
            response.status()
        );
        let wait_html = response.text().await?;
        ensure_session(&wait_html, &header)?;
        let mut request = self.client.get(OWN_WORK);
        if !header.is_empty() {
            request = request.header("Cookie", &header);
        }
        let response = request.send().await?;
        anyhow::ensure!(
            response.status().is_success(),
            "批改网已提交列表请求失败：HTTP {}",
            response.status()
        );
        let own_html = response.text().await?;
        let mut tasks = parse_wait_complete_at(&wait_html, Local::now().naive_local());
        let mut known = tasks
            .iter()
            .map(|task| task.task_id.clone())
            .collect::<std::collections::HashSet<_>>();
        for task in parse_own_work_at(&own_html, Local::now().naive_local()) {
            if known.insert(task.task_id.clone()) {
                tasks.push(task);
            }
        }
        let profile = self.profile();
        let tasks_count = tasks.len() as i64;
        let progress = (100 - (tasks_count * 10).min(100)).max(0);
        let account = AccountInfo {
            platform: "pigai".to_owned(),
            username: self.account.username.clone(),
            display_name: text(&profile, "u2", &self.account.username),
            user_id: text(&profile, "i", ""),
            school: text(&profile, "s", ""),
            extra: json!({
                "student_no": profile.get("no").cloned().unwrap_or(Value::Null),
                "class_id": profile.get("cl").cloned().unwrap_or(Value::Null),
                "points": profile.get("k").cloned().unwrap_or(Value::Null),
            }),
        };
        Ok(RefreshResult {
            courses: vec![CourseSnapshot {
                platform: "pigai".to_owned(),
                id: "pigai:all".to_owned(),
                name: "批改网作业".to_owned(),
                progress,
                raw: json!({"pending": tasks_count}),
            }],
            tasks,
            account: Some(account),
            warning: String::new(),
            session: self.session.to_value(),
        })
    }

    async fn lookup_essay(&mut self, essay_id: &str) -> anyhow::Result<TaskSnapshot> {
        anyhow::ensure!(
            !essay_id.is_empty() && essay_id.bytes().all(|byte| byte.is_ascii_digit()),
            "作文号必须是数字"
        );
        if !self.session.has_login_cookie() {
            self.login().await?;
        }
        let header = self.session.cookie_header();
        let mut request = self.client.get(format!("{BASE}/index.php")).query(&[
            ("c", "write"),
            ("a", "searchReq"),
            ("rid", essay_id),
        ]);
        if !header.is_empty() {
            request = request.header("Cookie", &header);
        }
        let response = request.send().await?;
        anyhow::ensure!(
            response.status().is_success(),
            "批改网作文请求失败：HTTP {}",
            response.status()
        );
        let final_url = response.url().to_string();
        let html = response.text().await?;
        ensure_session(&html, &header)?;
        parse_essay_page_at(&html, essay_id, &final_url, Local::now().naive_local())
            .ok_or_else(|| anyhow::anyhow!("没有找到作文号 {essay_id}，可能不存在或无权访问"))
    }
}

impl PigaiAdapter {
    async fn login(&mut self) -> anyhow::Result<()> {
        let username = self.account.username.clone();
        let password = self.password.clone();
        let session =
            tokio::task::spawn_blocking(move || login_with_system_browser(&username, &password))
                .await
                .map_err(|error| anyhow::anyhow!("浏览器登录任务失败：{error}"))??;
        anyhow::ensure!(session.has_login_cookie(), "批改网登录失败，请检查账号密码");
        self.session = session;
        Ok(())
    }
}

fn text(value: &Value, key: &str, fallback: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .unwrap_or(fallback)
        .to_owned()
}

fn ensure_session(html: &str, header: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !(html.contains("用户登录") && !header.contains("_JUKU_USER")),
        "批改网登录态已失效"
    );
    Ok(())
}

fn login_with_system_browser(username: &str, password: &str) -> anyhow::Result<PigaiSession> {
    use headless_chrome::{
        Browser, LaunchOptions, protocol::cdp::Page::CaptureScreenshotFormatOption,
    };
    let path =
        system_chrome_path().ok_or_else(|| anyhow::anyhow!("未检测到已安装的 Chrome 或 Edge"))?;
    let options = LaunchOptions::default_builder()
        .path(Some(path))
        .headless(true)
        .build()
        .map_err(|error| anyhow::anyhow!("无法启动系统浏览器：{error}"))?;
    let browser = Browser::new(options)?;
    let page = {
        let tabs = browser
            .get_tabs()
            .lock()
            .map_err(|_| anyhow::anyhow!("无法读取浏览器标签页"))?;
        tabs.first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("系统浏览器未创建默认标签页"))?
    };
    page.navigate_to(BASE)?;
    fill(&page, "#username", username)?;
    fill(&page, "#password", password)?;
    page.wait_for_element("#ulogin")?.click()?;
    std::thread::sleep(std::time::Duration::from_secs(4));
    let _ = page.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true);
    let cookies = page.get_cookies()?;
    let session = PigaiSession {
        version: 1,
        cookies: cookies
            .into_iter()
            .filter(|cookie| cookie.domain.contains("pigai.org"))
            .map(|cookie| PigaiCookie {
                name: cookie.name,
                value: cookie.value,
                domain: cookie.domain,
                path: cookie.path,
            })
            .collect(),
        uses_chromium_login: true,
    };
    Ok(session)
}

fn fill(
    page: &Arc<headless_chrome::browser::tab::Tab>,
    selector: &str,
    value: &str,
) -> anyhow::Result<()> {
    let selector = serde_json::to_string(selector)?;
    let value = serde_json::to_string(value)?;
    page.evaluate(
        &format!("document.querySelector({selector}).value = {value}; true"),
        true,
    )?;
    Ok(())
}

fn system_chrome_path() -> Option<std::path::PathBuf> {
    let candidates = [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    ];
    std::env::var_os("STUDYPILOT_CHROMIUM")
        .map(std::path::PathBuf::from)
        .filter(|path| path.exists())
        .or_else(|| {
            candidates
                .into_iter()
                .map(std::path::PathBuf::from)
                .find(|path| path.exists())
        })
}

fn parse_at(date: &str, now: NaiveDateTime) -> Option<String> {
    parse_utils::parse_deadline_at(date, now)
}

fn select_text(document: &Html, selector: &str) -> String {
    Selector::parse(selector)
        .ok()
        .and_then(|selector| document.select(&selector).next())
        .map(|node| node.text().collect::<Vec<_>>().join(" "))
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

fn select_attr(document: &Html, selector: &str, name: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    document
        .select(&selector)
        .find_map(|node| node.value().attr(name).map(str::to_owned))
}

pub fn parse_wait_complete_at(html: &str, now: NaiveDateTime) -> Vec<TaskSnapshot> {
    let document = Html::parse_document(html);
    let Some(container) = Selector::parse("#essayList")
        .ok()
        .and_then(|selector| document.select(&selector).next())
    else {
        return Vec::new();
    };
    let rows: Vec<_> = container
        .select(&Selector::parse("tr").expect("valid tr selector"))
        .collect();
    let mut tasks = Vec::new();
    for row in rows {
        let cells = row
            .select(&Selector::parse("th,td").expect("valid cells selector"))
            .map(|cell| {
                cell.text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>();
        if cells.len() < 5 || !cells[0].bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let links = row
            .select(&Selector::parse("a[href]").expect("valid links selector"))
            .collect::<Vec<_>>();
        let href = links
            .iter()
            .find_map(|link| {
                let href = link.value().attr("href")?;
                (href.contains("a=writeFrom") || href.contains("c=write"))
                    .then_some(href.to_owned())
            })
            .or_else(|| {
                links
                    .first()
                    .and_then(|link| link.value().attr("href").map(str::to_owned))
            })
            .unwrap_or_default();
        let href = if href.starts_with('/') {
            format!("{BASE}{href}")
        } else {
            href
        };
        let deadline = parse_at(&cells[4], now);
        tasks.push(TaskSnapshot {
            platform: "pigai".to_owned(),
            course_id: "pigai:all".to_owned(),
            course_name: "批改网作业".to_owned(),
            title: cells[1].clone(),
            task_id: cells[0].clone(),
            status: "pending".to_owned(),
            deadline,
            url: href,
            detail: format!("{} · 截止 {}", cells[2], cells[4]),
            source: "wait-complete".to_owned(),
            raw: json!({"cells": cells}),
        });
    }
    tasks
}

pub fn parse_own_work_at(html: &str, now: NaiveDateTime) -> Vec<TaskSnapshot> {
    let document = Html::parse_document(html);
    let Some(container) = Selector::parse("#essayList")
        .ok()
        .and_then(|selector| document.select(&selector).next())
    else {
        return Vec::new();
    };
    let mut tasks = Vec::new();
    for row in container.select(&Selector::parse("tr").expect("valid tr selector")) {
        let cells = row
            .select(&Selector::parse("th,td").expect("valid cells selector"))
            .map(|cell| {
                cell.text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>();
        if cells.len() < 6 || !cells[0].bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let deadline = parse_at(&cells[4], now);
        let status_text = &cells[5];
        if !status_text.contains("未截止") && deadline.is_none() {
            continue;
        }
        let href = row
            .select(&Selector::parse("a[href]").expect("valid links selector"))
            .find_map(|link| {
                let href = link.value().attr("href")?;
                (href.contains("a=view") || href.contains("a=writeFrom")).then_some(href.to_owned())
            })
            .unwrap_or_default();
        let href = if href.starts_with('/') {
            format!("{BASE}{href}")
        } else {
            href
        };
        tasks.push(TaskSnapshot {
            platform: "pigai".to_owned(),
            course_id: "pigai:all".to_owned(),
            course_name: "批改网作业".to_owned(),
            title: cells[1].clone(),
            task_id: cells[0].clone(),
            status: if status_text.contains("未截止") || status_text.is_empty() {
                "pending".to_owned()
            } else {
                "completed".to_owned()
            },
            deadline,
            url: href,
            detail: format!(
                "{} · {}",
                cells[2],
                if status_text.is_empty() {
                    "已提交"
                } else {
                    status_text
                }
            ),
            source: "own-work".to_owned(),
            raw: json!({"cells": cells}),
        });
    }
    tasks
}

fn extract_teacher(request: &str) -> String {
    regex::Regex::new(r"教师[:：]\s*([^\s]+)")
        .ok()
        .and_then(|regex| regex.captures(request))
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_owned())
        .unwrap_or_default()
}

pub fn parse_essay_page_at(
    html: &str,
    essay_id: &str,
    url: &str,
    now: NaiveDateTime,
) -> Option<TaskSnapshot> {
    let document = Html::parse_document(html);
    let title = select_attr(&document, "#title", "value")
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            let value = select_text(&document, "#timu");
            let value = value.trim_start_matches("题目：").trim().to_owned();
            (!value.is_empty()).then_some(value)
        })
        .unwrap_or_else(|| format!("第{essay_id}号 作文"));
    let deadline_text = document
        .select(&Selector::parse(".contentsW_bar_txt, #contentsW_bar_txt").ok()?)
        .map(|node| {
            node.text()
                .collect::<Vec<_>>()
                .join(" ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .find(|text| text.contains("截止时间"))
        .unwrap_or_default();
    let deadline = parse_at(&deadline_text, now);
    let request_text = select_text(&document, "#v2_request_titlex");
    let teacher = extract_teacher(&request_text);
    let has_marker = Selector::parse("#timu, #title, #v2_request_titlex")
        .ok()
        .is_some_and(|selector| document.select(&selector).next().is_some());
    if !has_marker {
        return None;
    }
    let status = deadline
        .as_deref()
        .and_then(|value| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M").ok())
        .is_some_and(|deadline| deadline < now);
    Some(TaskSnapshot {
        platform: "pigai".to_owned(),
        course_id: "pigai:all".to_owned(),
        course_name: "批改网作业".to_owned(),
        title,
        task_id: essay_id.to_owned(),
        status: if status { "completed" } else { "pending" }.to_owned(),
        deadline,
        url: if url.is_empty() {
            format!("{BASE}/index.php?c=v2&a=write&rid={essay_id}")
        } else {
            url.to_owned()
        },
        detail: format!(
            "作文号 {essay_id}{}",
            if teacher.is_empty() {
                String::new()
            } else {
                format!(" · 教师 {teacher}")
            }
        ),
        source: "essay-lookup".to_owned(),
        raw: json!({
            "essay_id": essay_id,
            "request": request_text,
            "deadline_text": deadline_text,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 16)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
    }

    #[test]
    fn parses_wait_complete_fixture() {
        let tasks = parse_wait_complete_at(
            include_str!("../../tests/fixtures/pigai/wait-complete.html"),
            now(),
        );
        assert_eq!(
            serde_json::to_value(tasks).unwrap(),
            json!([{
                "platform": "pigai",
                "course_id": "pigai:all",
                "course_name": "批改网作业",
                "title": "The Value of Teamwork",
                "task_id": "2311479",
                "status": "pending",
                "deadline": "2026-12-01 23:59",
                "url": "https://www.pigai.org/index.php?c=write&a=writeFrom&rid=2311479",
                "detail": "王老师 · 截止 2026-12-01 23:59",
                "source": "wait-complete",
                "raw": {
                    "cells": [
                        "2311479",
                        "The Value of Teamwork",
                        "王老师",
                        "2026级1班",
                        "2026-12-01 23:59",
                        "开始写作"
                    ]
                }
            }])
        );
    }

    #[test]
    fn parses_empty_wait_complete_fixture() {
        assert!(
            parse_wait_complete_at(
                include_str!("../../tests/fixtures/pigai/wait-complete-empty.html"),
                now()
            )
            .is_empty()
        );
    }

    #[test]
    fn parses_own_work_and_keeps_open_deadlines() {
        let html = include_str!("../../tests/fixtures/pigai/own-work.html");
        let tasks = parse_own_work_at(html, now());
        assert_eq!(
            serde_json::to_value(tasks).unwrap(),
            json!([{
                "platform": "pigai",
                "course_id": "pigai:all",
                "course_name": "批改网作业",
                "title": "My Campus Life",
                "task_id": "2311480",
                "status": "pending",
                "deadline": "2026-12-02 23:59",
                "url": "https://www.pigai.org/index.php?c=write&a=view&rid=2311480",
                "detail": "李老师 · 未截止",
                "source": "own-work",
                "raw": {
                    "cells": [
                        "2311480",
                        "My Campus Life",
                        "李老师",
                        "2026级2班",
                        "2026-12-02 23:59",
                        "未截止",
                        "查看"
                    ]
                }
            }])
        );
    }

    #[test]
    fn parses_essay_page_and_rejects_unrelated_html() {
        let html = include_str!("../../tests/fixtures/pigai/essay.html");
        let task = parse_essay_page_at(
            html,
            "2311479",
            "https://www.pigai.org/index.php?c=write&a=searchReq&rid=2311479",
            now(),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(task).unwrap(),
            json!({
                "platform": "pigai",
                "course_id": "pigai:all",
                "course_name": "批改网作业",
                "title": "The Value of Teamwork",
                "task_id": "2311479",
                "status": "pending",
                "deadline": "2026-12-01 23:59",
                "url": "https://www.pigai.org/index.php?c=write&a=searchReq&rid=2311479",
                "detail": "作文号 2311479 · 教师 王老师",
                "source": "essay-lookup",
                "raw": {
                    "essay_id": "2311479",
                "request": "教师：王老师 班级：2026级1班",
                    "deadline_text": "截止时间：2026-12-01 23:59"
                }
            })
        );
        assert!(
            parse_essay_page_at("<html><body>无相关内容</body></html>", "1", "", now()).is_none()
        );
    }

    #[test]
    fn validates_profile_cookie() {
        let profile = json!({"u2":"张三","i":"1001","s":"测试大学","no":"2026001","cl":"3","k":42});
        let session = PigaiSession {
            version: 1,
            cookies: vec![PigaiCookie {
                name: "_JUKU_USER".to_owned(),
                value: format!(
                    "={}",
                    url::form_urlencoded::byte_serialize(profile.to_string().as_bytes())
                        .collect::<String>()
                ),
                domain: "www.pigai.org".to_owned(),
                path: "/".to_owned(),
            }],
            uses_chromium_login: true,
        };
        assert_eq!(session.profile()["u2"], "张三");
        assert!(session.cookie_header().contains("_JUKU_USER="));
    }
}
