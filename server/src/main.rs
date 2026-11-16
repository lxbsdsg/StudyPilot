use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::PathBuf,
    time::Duration,
};

use studypilot_core::{
    api,
    config::data_dir,
    reminder_runner::{DEFAULT_SCAN_INTERVAL, run_reminder_runner},
};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 17890;
const PORT_END: u16 = 17930;

struct Config {
    data_dir: PathBuf,
    host: IpAddr,
    port: u16,
    open_browser: bool,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        let mut args = std::env::args().skip(1);
        let mut host = DEFAULT_HOST.to_owned();
        let mut port = DEFAULT_PORT;
        let mut open_browser = true;
        while let Some(argument) = args.next() {
            match argument.as_str() {
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
                        "StudyPilot Rust server v2\n\
                         \n\
                         Usage: studypilot-server-v2 [--host 127.0.0.1] \
                         [--port PORT] [--no-browser]"
                    );
                    std::process::exit(0);
                }
                _ => anyhow::bail!("未知参数：{argument}"),
            }
        }
        let host = host.parse::<IpAddr>()?;
        anyhow::ensure!(
            host == IpAddr::V4(Ipv4Addr::LOCALHOST) || host == IpAddr::V6(Ipv6Addr::LOCALHOST),
            "服务只能绑定本机地址"
        );
        Ok(Self {
            data_dir: data_dir()?,
            host,
            port,
            open_browser,
        })
    }
}

fn pick_port(host: IpAddr, preferred: u16) -> u16 {
    for port in std::iter::once(preferred).chain(DEFAULT_PORT..PORT_END) {
        if std::net::TcpListener::bind((host, port)).is_ok() {
            return port;
        }
    }
    0
}

fn open_browser(url: &str) {
    let mut command = if cfg!(windows) {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };
    if let Err(error) = command.spawn() {
        tracing::warn!("无法打开浏览器：{error}");
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "studypilot_server_v2=info".into()),
        )
        .init();
    let config = Config::from_env()?;
    let store = studypilot_core::storage::StoreInner::open(config.data_dir.join("studypilot.db"))?;
    let router = api::default_router(store);
    let _reminders = run_reminder_runner(
        config.data_dir.clone(),
        DEFAULT_SCAN_INTERVAL,
        |reminder| async move {
            tracing::info!(
                "提醒已触发：{} ({})",
                reminder["title"].as_str().unwrap_or("未命名提醒"),
                reminder["remind_at"].as_str().unwrap_or("")
            );
            Ok(())
        },
    );
    let port = pick_port(config.host, config.port);
    let listener = tokio::net::TcpListener::bind((config.host, port)).await?;
    let port = listener.local_addr()?.port();
    let url = format!("http://{}:{port}/", config.host);
    tracing::info!("StudyPilot Rust v2 listening at {url}");
    if config.open_browser {
        let url = url.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(800));
            open_browser(&url);
        });
    }
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
