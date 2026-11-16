use serde::Serialize;

const MOBILE_MARKERS: [&str; 5] = ["android", "iphone", "ipad", "ipod", "mobile"];

fn platform_targets(platform: &str) -> Option<(&'static str, &'static str)> {
    match platform {
        "pigai" => Some(("https://www.pigai.org/", "https://www.pigai.org/zt/mobile/")),
        "unipus" => Some(("https://uai.unipus.cn/", "https://uai.unipus.cn/")),
        "welearn" => Some((
            "https://welearn.sflep.com/2019/student/index.aspx",
            "https://welearn.sflep.com/2019/student/index.aspx",
        )),
        "chaoxing" => Some(("https://mooc1.chaoxing.com/", "https://mooc1.chaoxing.com/")),
        _ => None,
    }
}

pub fn is_mobile_user_agent(user_agent: &str) -> bool {
    let text = user_agent.to_lowercase();
    MOBILE_MARKERS.iter().any(|marker| text.contains(marker))
}

pub fn desktop_url(platform: &str, _task_url: &str) -> String {
    platform_targets(platform)
        .map(|(desktop, _)| desktop)
        .unwrap_or_default()
        .to_owned()
}

#[derive(Debug, Serialize, PartialEq)]
pub struct MobileTarget {
    pub kind: &'static str,
    pub platform: String,
    pub web_url: String,
    pub desktop_url: String,
}

pub fn mobile_targets(platform: &str, task_url: &str) -> MobileTarget {
    let (desktop, mobile) = platform_targets(platform).unwrap_or(("", ""));
    MobileTarget {
        kind: "mobile",
        platform: platform.to_owned(),
        web_url: if mobile.is_empty() { desktop } else { mobile }.to_owned(),
        desktop_url: desktop_url(platform, task_url),
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TaskOpenTarget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_url: Option<String>,
    pub platform: String,
    pub task_id: String,
    pub title: String,
}

pub fn task_open_target(
    platform: &str,
    task_id: &str,
    title: &str,
    task_url: &str,
    mobile: bool,
) -> TaskOpenTarget {
    if mobile {
        let target = mobile_targets(platform, task_url);
        TaskOpenTarget {
            kind: Some(target.kind),
            url: None,
            web_url: Some(target.web_url),
            desktop_url: Some(target.desktop_url),
            platform: platform.to_owned(),
            task_id: task_id.to_owned(),
            title: title.to_owned(),
        }
    } else {
        TaskOpenTarget {
            kind: Some("website"),
            url: Some(desktop_url(platform, task_url)),
            web_url: None,
            desktop_url: None,
            platform: platform.to_owned(),
            task_id: task_id.to_owned(),
            title: title.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_mobile_user_agents() {
        assert!(is_mobile_user_agent(
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0)"
        ));
        assert!(!is_mobile_user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"
        ));
    }

    #[test]
    fn does_not_follow_task_urls() {
        assert_eq!(
            desktop_url("pigai", "https://www.pigai.org/essay/1"),
            "https://www.pigai.org/"
        );
        assert_eq!(
            mobile_targets("pigai", "https://www.pigai.org/essay/1"),
            MobileTarget {
                kind: "mobile",
                platform: "pigai".to_owned(),
                web_url: "https://www.pigai.org/zt/mobile/".to_owned(),
                desktop_url: "https://www.pigai.org/".to_owned(),
            }
        );
    }
}
