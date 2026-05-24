function Get-ComfyGitExecutable {
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

    $moduleDir = $PSScriptRoot
    if ($moduleDir) {
        $nextToModule = Join-Path $moduleDir 'ComfyGit'
        if (Test-Path -LiteralPath $nextToModule) {
            return $nextToModule
        }
        $nextToModuleExe = Join-Path $moduleDir 'ComfyGit.exe'
        if (Test-Path -LiteralPath $nextToModuleExe) {
            return $nextToModuleExe
        }
    }

    $cmd = Get-Command ComfyGit -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($cmd) {
        return $cmd.Source
    }

    return 'ComfyGit'
}

function cg {
    $Arguments = $args
    $comfygitExe = Get-ComfyGitExecutable

    if ($Arguments.Count -gt 0 -and $Arguments[0] -eq 'cd') {
        if ($Arguments.Count -lt 2 -or $Arguments.Count -gt 3) {
            Write-Error 'usage: cg cd <alias> [sub]'
            return
        }

        if ($Arguments.Count -eq 3) {
            $targetDir = & $comfygitExe pwd $Arguments[1] $Arguments[2]
        } else {
            $targetDir = & $comfygitExe pwd $Arguments[1]
        }
        if ($LASTEXITCODE -ne 0) {
            return
        }

        Set-Location -LiteralPath $targetDir
        return
    }

    & $comfygitExe @Arguments
}

Export-ModuleMember -Function cg
