$Arguments = $args

function Get-ComfyGitLauncher {
    if ($env:COMFYGIT_EXE -and (Test-Path -LiteralPath $env:COMFYGIT_EXE)) {
        return $env:COMFYGIT_EXE
    }
    $localLauncher = Join-Path $HOME '.local/bin/ComfyGit'
    if (Test-Path -LiteralPath $localLauncher) {
        return $localLauncher
    }
    $exeNextToScript = Join-Path $PSScriptRoot 'ComfyGit.exe'
    if (Test-Path -LiteralPath $exeNextToScript) {
        return $exeNextToScript
    }
    $unixNextToScript = Join-Path $PSScriptRoot 'ComfyGit'
    if (Test-Path -LiteralPath $unixNextToScript) {
        return $unixNextToScript
    }
    $cmd = Get-Command ComfyGit -CommandType Application -ErrorAction SilentlyContinue
    if ($cmd) {
        return $cmd.Source
    }
    return 'ComfyGit'
}

$cgBin = Get-ComfyGitLauncher

if ($Arguments.Count -gt 0 -and $Arguments[0] -eq 'cd') {
    if ($Arguments.Count -ne 2) {
        Write-Error 'usage: cg cd <alias>'
        exit 2
    }

    $targetDir = & $cgBin pwd $Arguments[1]
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    Set-Location -LiteralPath $targetDir
    exit 0
}

& $cgBin @Arguments
exit $LASTEXITCODE