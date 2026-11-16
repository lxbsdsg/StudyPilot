@echo off
setlocal
cd /d "%~dp0"

where npm >nul 2>&1
if errorlevel 1 (
    echo Node.js and npm are required to run the Tauri desktop application.
    exit /b 1
)

if not exist "frontend\node_modules" (
    pushd frontend
    call npm install
    if errorlevel 1 (
        popd
        exit /b 1
    )
    popd
)

call npm run desktop:dev
exit /b %ERRORLEVEL%
