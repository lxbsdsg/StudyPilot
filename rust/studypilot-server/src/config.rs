use std::path::PathBuf;

pub const VERSION: &str = "0.1.0";
pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 17890;
pub const PORT_END: u16 = 17930;

pub const PLATFORM_LABELS: [(&str, &str); 4] = [
    ("unipus", "U校园"),
    ("welearn", "WELearn"),
    ("chaoxing", "学习通"),
    ("pigai", "批改网"),
];

pub fn platform_label(platform: &str) -> &str {
    PLATFORM_LABELS
        .iter()
        .find_map(|(id, label)| (*id == platform).then_some(*label))
        .unwrap_or(platform)
}

pub fn data_dir() -> anyhow::Result<PathBuf> {
    let path = match std::env::var_os("STUDYPILOT_DATA_DIR") {
        Some(value) => PathBuf::from(value),
        None => std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("无法确定用户主目录"))?
            .join(".studypilot"),
    };
    Ok(path)
}

pub struct Config {
    pub data_dir: PathBuf,
    pub host: String,
    pub port: u16,
    pub open_browser: bool,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let mut args = std::env::args().skip(1);
        let mut host = DEFAULT_HOST.to_owned();
        let mut port = DEFAULT_PORT;
        let mut open_browser = true;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--host" => {
                    host = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("--host 缺少参数"))?;
                }
                "--port" => {
                    port = args
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("--port 缺少参数"))?
                        .parse()?;
                }
                "--no-browser" => open_browser = false,
                "--help" | "-h" => {
                    println!(
                        "StudyPilot Rust server\n\
                         \n\
                         Usage: studypilot-server [--host HOST] [--port PORT] [--no-browser]"
                    );
                    std::process::exit(0);
                }
                _ => anyhow::bail!("未知参数：{arg}"),
            }
        }
        Ok(Self {
            data_dir: data_dir()?,
            host,
            port,
            open_browser,
        })
    }
}
