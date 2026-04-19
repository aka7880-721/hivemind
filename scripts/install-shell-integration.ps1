param(
    [string]$ShellAssetDir = "$PSScriptRoot/../assets/shell",
    [string]$BinDir
)

$ErrorActionPreference = 'Stop'

if (-not $BinDir) {
    $cgBinCommand = Get-Command cg-bin -ErrorAction SilentlyContinue
    if ($cgBinCommand) {
        $BinDir = Split-Path $cgBinCommand.Source -Parent
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
$legacyExe = Join-Path $BinDir 'cg.exe'
$delegateExe = Join-Path $BinDir 'cg-bin.exe'

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
Copy-Item -Path $cgPs1Source -Destination $cgPs1Target -Force
Copy-Item -Path $cgCmdSource -Destination $cgCmdTarget -Force
New-Item -ItemType Directory -Force -Path $cgModuleRoot | Out-Null
Copy-Item -Path $cgModuleSource -Destination $cgModuleTarget -Force

if ((Test-Path $legacyExe) -and (Test-Path $delegateExe)) {
    Remove-Item -Force -Path $legacyExe
}

Write-Host "Installed ComfyGit PowerShell wrapper to $cgPs1Target" -ForegroundColor Green
Write-Host "Installed ComfyGit cmd wrapper to $cgCmdTarget" -ForegroundColor Green
Write-Host "Installed ComfyGit PowerShell module to $cgModuleTarget" -ForegroundColor Green
if (Test-Path $delegateExe) {
    Write-Host "Removed legacy cg.exe so wrapper commands take precedence over the compiled binary." -ForegroundColor Yellow
}
Write-Host "Open a new PowerShell or cmd session to enable real 'cg cd <alias>' support." -ForegroundColor Green