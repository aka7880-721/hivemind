function cg {
    $Arguments = $args

    if ($Arguments.Count -gt 0 -and $Arguments[0] -eq 'cd') {
        if ($Arguments.Count -ne 2) {
            Write-Error 'usage: cg cd <alias>'
            return
        }

        $targetDir = & cg-bin pwd $Arguments[1]
        if ($LASTEXITCODE -ne 0) {
            return
        }

        Set-Location -LiteralPath $targetDir
        return
    }

    & cg-bin @Arguments
}

Export-ModuleMember -Function cg