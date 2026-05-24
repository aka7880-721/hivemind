function cg {
    $Arguments = $args

    if ($Arguments.Count -gt 0 -and $Arguments[0] -eq 'cd') {
        if ($Arguments.Count -lt 2 -or $Arguments.Count -gt 3) {
            Write-Error 'usage: cg cd <alias> [sub]'
            return
        }

        if ($Arguments.Count -eq 3) {
            $targetDir = & ComfyGit pwd $Arguments[1] $Arguments[2]
        } else {
            $targetDir = & ComfyGit pwd $Arguments[1]
        }
        if ($LASTEXITCODE -ne 0) {
            return
        }

        Set-Location -LiteralPath $targetDir
        return
    }

    & ComfyGit @Arguments
}

Export-ModuleMember -Function cg