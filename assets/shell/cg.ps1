$Arguments = $args

function Get-ComfyGitLauncher {
    if ($env:COMFYGIT_EXE -and (Test-Path -LiteralPath $env:COMFYGIT_EXE)) {
        return $env:COMFYGIT_EXE
    }

    $candidates = @(
        (Join-Path $HOME '.local/bin/ComfyGit')
        '/usr/local/bin/ComfyGit'
        '/usr/bin/ComfyGit'
        '/usr/local/sbin/ComfyGit'
    )
    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path -LiteralPath $candidate)) {
            return $candidate
        }
    }

    $exeNextToScript = Join-Path $PSScriptRoot 'ComfyGit.exe'
    if (Test-Path -LiteralPath $exeNextToScript) {
        return $exeNextToScript
    }
    $unixNextToScript = Join-Path $PSScriptRoot 'ComfyGit'
    if (Test-Path -LiteralPath $unixNextToScript) {
        return $unixNextToScript
    }

    $cmd = Get-Command ComfyGit -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($cmd) {
        return $cmd.Source
    }

    return 'ComfyGit'
}

$cgBin = Get-ComfyGitLauncher

if ($Arguments.Count -gt 0 -and $Arguments[0] -eq 'cd') {
    if ($Arguments.Count -lt 2 -or $Arguments.Count -gt 3) {
        Write-Error 'usage: cg cd <alias> [sub]'
        exit 2
    }

    if ($Arguments.Count -eq 3) {
        $targetDir = & $cgBin pwd $Arguments[1] $Arguments[2]
    } else {
        $targetDir = & $cgBin pwd $Arguments[1]
    }
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    Set-Location -LiteralPath $targetDir
    exit 0
}

& $cgBin @Arguments
exit $LASTEXITCODE
