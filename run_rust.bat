@echo off
setlocal
cd /d "%~dp0"

if exist "rust\studypilot-server\target\release\studypilot-server.exe" (
    start "" "rust\studypilot-server\target\release\studypilot-server.exe" %*
    exit /b 0
)

if exist "rust\studypilot-server\target\debug\studypilot-server.exe" (
    start "" "rust\studypilot-server\target\debug\studypilot-server.exe" %*
    exit /b 0
)

where cargo >nul 2>&1
if errorlevel 1 (
    echo Rust toolchain not found. Install rustup or build the release binary first.
    exit /b 1
)

pushd rust\studypilot-server
cargo run --release -- %*
set "EXIT_CODE=%ERRORLEVEL%"
popd
exit /b %EXIT_CODE%
