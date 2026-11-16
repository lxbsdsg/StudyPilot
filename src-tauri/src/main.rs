#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = studypilot_desktop::run() {
        eprintln!("StudyPilot 启动失败：{error:#}");
        std::process::exit(1);
    }
}
