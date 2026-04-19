@echo off
setlocal EnableExtensions EnableDelayedExpansion

set "CG_BIN=%~dp0ComfyGit.exe"
if not exist "%CG_BIN%" set "CG_BIN=ComfyGit.exe"

if /I "%~1"=="cd" (
    if "%~2"=="" (
        echo usage: cg cd ^<alias^> 1>&2
        exit /b 2
    )

    for /f "usebackq delims=" %%I in (`"%CG_BIN%" pwd "%~2"`) do set "CG_TARGET_DIR=%%~I"
    if errorlevel 1 exit /b %errorlevel%
    if not defined CG_TARGET_DIR exit /b 1
    endlocal & cd /d "%CG_TARGET_DIR%"
    exit /b %errorlevel%
)

endlocal & "%CG_BIN%" %*