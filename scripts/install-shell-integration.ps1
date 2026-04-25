param(
    [string]$ShellAssetDir = "$PSScriptRoot/../assets/shell",
    [string]$BinDir
)

$ErrorActionPreference = 'Stop'

if (-not $BinDir) {
    $cgCommand = Get-Command ComfyGit -ErrorAction SilentlyContinue
    if ($cgCommand) {
        $BinDir = Split-Path $cgCommand.Source -Parent
    } else {
        $BinDir = Join-Path $HOME '.cargo/bin'
    }
}

$cgPs1Source = Join-Path $ShellAssetDir 'cg.ps1'
$cgCmdSource = Join-Path $ShellAssetDir 'cg.cmd'
$cgModuleSource = Join-Path $ShellAssetDir 'ComfyGit.psm1'
$cgPs1Target = Join-Path $BinDir 'cg.ps1'
$cgCmdTarget = Join-Path $BinDir 'cg.cmd'
$cgModuleRoot = Join-Path $HOME 'Documents/PowerShell/Modules/ComfyGit'
$cgModuleTarget = Join-Path $cgModuleRoot 'ComfyGit.psm1'
$cgCmdInitRoot = Join-Path $env:LOCALAPPDATA 'ComfyGit'
$cgCmdInitTarget = Join-Path $cgCmdInitRoot 'cg-cmd-init.cmd'
$cgCmdPwdTarget = Join-Path $cgCmdInitRoot 'cg-cmd-pwd.ps1'
$cmdAutoRunKey = 'HKCU:\Software\Microsoft\Command Processor'
$legacyExe = Join-Path $BinDir 'cg.exe'
$delegateExe = Join-Path $BinDir 'ComfyGit.exe'

function Register-ComfyGitCmdAutoRun {
    param(
        [string]$DelegateExePath,
        [string]$InitFilePath,
        [string]$PwdHelperPath
    )

    New-Item -ItemType Directory -Force -Path (Split-Path $InitFilePath -Parent) | Out-Null

    $escapedDelegateExePath = $DelegateExePath.Replace("'", "''")
    $pwdHelperContent = @'
param([string]$AliasName)
& '{0}' pwd $AliasName
'@ -f $escapedDelegateExePath
    Set-Content -Path $PwdHelperPath -Value $pwdHelperContent -Encoding UTF8

    $initContent = @'
@echo off
doskey cg=if "$1"=="cd" (for /f "usebackq delims=" %%I in (`powershell -NoProfile -ExecutionPolicy Bypass -File "{0}" "$2"`) do @cd /d "%%~I") $T if not "$1"=="cd" cg.cmd $*
'@ -f $PwdHelperPath
    Set-Content -Path $InitFilePath -Value $initContent -Encoding ASCII

    New-Item -Path $cmdAutoRunKey -Force | Out-Null
    $snippet = "if exist `"$InitFilePath`" call `"$InitFilePath`""
    $existing = (Get-ItemProperty -Path $cmdAutoRunKey -Name AutoRun -ErrorAction SilentlyContinue).AutoRun

    if ([string]::IsNullOrWhiteSpace($existing)) {
        Set-ItemProperty -Path $cmdAutoRunKey -Name AutoRun -Value $snippet
        return
    }

    if ($existing -notlike "*${InitFilePath}*") {
        Set-ItemProperty -Path $cmdAutoRunKey -Name AutoRun -Value "$existing & $snippet"
    }
}

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
Copy-Item -Path $cgPs1Source -Destination $cgPs1Target -Force
Copy-Item -Path $cgCmdSource -Destination $cgCmdTarget -Force
New-Item -ItemType Directory -Force -Path $cgModuleRoot | Out-Null
Copy-Item -Path $cgModuleSource -Destination $cgModuleTarget -Force

if ((Test-Path $legacyExe) -and (Test-Path $delegateExe)) {
    Remove-Item -Force -Path $legacyExe
}

if (Test-Path $delegateExe) {
    Register-ComfyGitCmdAutoRun -DelegateExePath $delegateExe -InitFilePath $cgCmdInitTarget -PwdHelperPath $cgCmdPwdTarget
}

if (Test-Path $cgModuleTarget) {
    try {
        Import-Module -Force $cgModuleTarget -ErrorAction Stop
        Write-Host "Imported ComfyGit PowerShell module into this session." -ForegroundColor Green
    } catch {
        Write-Host "Installed the ComfyGit PowerShell module, but automatic import failed in this session." -ForegroundColor Yellow
    }
}

Write-Host "Installed ComfyGit PowerShell wrapper to $cgPs1Target" -ForegroundColor Green
Write-Host "Installed ComfyGit cmd wrapper to $cgCmdTarget" -ForegroundColor Green
Write-Host "Installed ComfyGit PowerShell module to $cgModuleTarget" -ForegroundColor Green
Write-Host "Installed ComfyGit cmd session hook to $cgCmdInitTarget" -ForegroundColor Green
Write-Host "Installed ComfyGit cmd path helper to $cgCmdPwdTarget" -ForegroundColor Green
if (Test-Path $delegateExe) {
    Write-Host "Removed legacy cg.exe so wrapper commands take precedence over the compiled binary." -ForegroundColor Yellow
}
Write-Host "Open a new PowerShell or cmd session to enable real 'cg cd <alias>' support." -ForegroundColor Green