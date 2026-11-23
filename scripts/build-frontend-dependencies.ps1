<#
.SYNOPSIS
Brings one frontend's node_modules to exactly its committed lockfile.

.DESCRIPTION
Package builds must bundle the dependency tree named by package-lock.json, not
whatever an earlier install left behind. This runs `npm ci` (a clean,
lockfile-enforcing install) unless node_modules carries a stamp proving it is
already the clean install of the current package.json and package-lock.json.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Project,
    [string]$Npm = "npm.cmd"
)

$ErrorActionPreference = "Stop"
$projectRoot = [IO.Path]::GetFullPath($Project)
$manifest = Join-Path $projectRoot "package.json"
$lockfile = Join-Path $projectRoot "package-lock.json"
foreach ($required in @($manifest, $lockfile)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "$required is required for a lockfile-enforced install"
    }
}

function Get-LockStamp {
    return (@($manifest, $lockfile) | ForEach-Object {
        (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
    }) -join " "
}

$stampPath = Join-Path $projectRoot "node_modules\.sgt-lock-sha256"
$expected = Get-LockStamp
if ((Test-Path -LiteralPath $stampPath -PathType Leaf) -and
    (Get-Content -LiteralPath $stampPath -Raw).Trim() -eq $expected) {
    return
}

Push-Location $projectRoot
try {
    & $Npm ci
    if ($LASTEXITCODE -ne 0) {
        throw "npm ci failed in $projectRoot"
    }
}
finally {
    Pop-Location
}
if ((Get-LockStamp) -ne $expected) {
    throw "package.json or package-lock.json changed during npm ci in $projectRoot"
}
New-Item -ItemType Directory -Path (Split-Path -Parent $stampPath) -Force | Out-Null
Set-Content -LiteralPath $stampPath -Value $expected -NoNewline -Encoding ascii
