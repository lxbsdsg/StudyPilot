# StudyPilot Rust 服务

本目录是 `rust-refactor` 分支中的渐进式 Rust 重构。Rust 服务使用与 Python
版相同的数据目录和数据库文件，并复用现有前端资源，因此可以读取已有课程、
任务、已完成记录、提醒和账号会话。

## 当前覆盖范围

- Axum 本地 HTTP 服务，默认监听 `127.0.0.1:17890`，端口冲突时自动换端口。
- 现有 `studypilot/web` 前端和账号、课程、任务、完成历史、提醒等主要 API。
- SQLite 表结构与 Python 版一致；不自动删表、不改已有数据。
- 账号密码使用 Windows DPAPI；非 Windows 平台使用同格式的 AES-GCM 与
  `secret.key`。
- 平台官网跳转、移动端 User-Agent 识别、截止时间解析、提醒时间计算和
  提醒 CRUD。
- 账号新增/删除、任务完成历史、恢复未完成和按截止时间创建提醒。
- Rust 发布构建可直接运行，不依赖本机 Python 运行时；旧版数据仍可直接读取。

## 尚未迁移

四个平台的登录与抓取适配器、作文号查询、批量通知和桌面窗口仍在 Python 版中。
Rust 服务中的账号刷新、批量刷新和作文号查询接口目前返回明确的未迁移提示，
不会伪造成功结果。后台提醒触发也尚未迁移。

因此，在完成适配器迁移前，建议继续使用 Python 版进行账号刷新。Rust 版本适合
验证前端、本地存储、提醒管理和 API 兼容性。

## 运行

从仓库根目录双击 `run_rust.bat`，或在 `rust/studypilot-server` 中执行：

```bat
cargo run --release -- --no-browser
```

默认浏览器模式：

```bat
cargo run --release
```

可用参数与 Python 版接近：

```text
--host 127.0.0.1
--port 17890
--no-browser
```

可通过 `STUDYPILOT_DATA_DIR` 指定测试数据目录。Windows 默认数据目录是
`%USERPROFILE%\.studypilot\`，其他平台使用 `~/.studypilot/`。不要同时运行
Python 和 Rust 两个服务。

## 测试

```bat
cd rust\studypilot-server
cargo test
cargo check
```
