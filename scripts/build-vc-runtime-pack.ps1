[CmdletBinding()]
param(
    [switch]$RequireDelivery,
    [string]$OutputDir
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$arguments = @((Join-Path $PSScriptRoot "package_vc_runtime.py"))
if (-not [string]::IsNullOrWhiteSpace($OutputDir)) {
    $arguments += @("--output-dir", [IO.Path]::GetFullPath($OutputDir))
}
if ($RequireDelivery) {
    $arguments += "--require-delivery"
}

Push-Location $repoRoot
try {
    & py -3 @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "VC runtime packaging failed"
    }
}
finally {
    Pop-Location
}
