function Get-SgtSourceFingerprint {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)

    Push-Location ([IO.Path]::GetFullPath($RepoRoot))
    try {
        $head = (& git rev-parse HEAD).Trim()
        if ($LASTEXITCODE -ne 0) { throw "Failed to resolve the source revision." }
        $tracked = (& git diff --binary --no-ext-diff HEAD | git hash-object --stdin).Trim()
        if ($LASTEXITCODE -ne 0) { throw "Failed to fingerprint tracked source changes." }
        $untracked = foreach ($path in @(& git ls-files --others --exclude-standard | Sort-Object)) {
            $digest = (& git hash-object --no-filters -- $path).Trim()
            if ($LASTEXITCODE -ne 0) {
                throw "Failed to fingerprint untracked source: $path"
            }
            "$path`t$digest"
        }
        $record = "head=$head`ntracked=$tracked`nuntracked=$($untracked -join "`n")"
        $bytes = [Text.Encoding]::UTF8.GetBytes($record)
        return [Convert]::ToHexString(
            [Security.Cryptography.SHA256]::HashData($bytes)
        ).ToLowerInvariant()
    }
    finally {
        Pop-Location
    }
}

function Get-SgtFileIdentity {
    param([Parameter(Mandatory = $true)][string]$Path)

    $file = Get-Item -LiteralPath $Path -ErrorAction Stop
    if (-not $file.PSIsContainer -and $file.Length -gt 0) {
        return [pscustomobject]@{
            path = $file.FullName
            bytes = $file.Length
            sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    throw "Expected a nonempty file: $Path"
}
