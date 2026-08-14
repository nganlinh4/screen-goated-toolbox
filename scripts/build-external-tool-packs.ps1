param(
    [switch]$RequireDelivery,
    [string]$OutputDir
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$arguments = @(
    (Join-Path $PSScriptRoot "package_external_tools.py")
)
if (-not [string]::IsNullOrWhiteSpace($OutputDir)) {
    $arguments += @("--output-dir", [IO.Path]::GetFullPath($OutputDir))
    $arguments += @("--audit-dir", (Join-Path ([IO.Path]::GetFullPath($OutputDir)) "audit"))
}
if ($RequireDelivery) {
    $arguments += "--require-delivery"
}
& py -3 @arguments
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
