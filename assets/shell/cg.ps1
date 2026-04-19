$Arguments = $args

$cgBin = Join-Path $PSScriptRoot 'ComfyGit.exe'
if (-not (Test-Path $cgBin)) {
    $cgBin = 'ComfyGit'
}

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