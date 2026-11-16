@echo off
setlocal
cd /d "%~dp0"

if exist "server\target\release\studypilot-server-v2.exe" (
    start "" "server\target\release\studypilot-server-v2.exe" %*
    exit /b 0
)

if exist "server\target\debug\studypilot-server-v2.exe" (
    start "" "server\target\debug\studypilot-server-v2.exe" %*
    exit /b 0
)

where cargo >nul 2>&1
if errorlevel 1 (
    echo Rust toolchain not found. Install rustup or build the release binary first.
    exit /b 1
)

pushd server
cargo run --release -- %*
set "EXIT_CODE=%ERRORLEVEL%"
popd
exit /b %EXIT_CODE%
