@echo off
setlocal EnableExtensions

set "CG_BIN=%~dp0ComfyGit.exe"
if not exist "%CG_BIN%" set "CG_BIN=ComfyGit.exe"

if /I "%~1"=="cd" (
    if "%~2"=="" (
        echo usage: cg cd ^<alias^> 1>&2
        exit /b 2
    )

    for /f "usebackq delims=" %%I in (`""%CG_BIN%" pwd "%~2""`) do (
        endlocal
        cd /d "%%~I"
        exit /b 0
    )
    exit /b %errorlevel%
)

"%CG_BIN%" %*
set "CG_EXIT=%errorlevel%"
endlocal & exit /b %CG_EXIT%