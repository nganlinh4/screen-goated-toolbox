param(
    [switch]$RequireDelivery
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$arguments = @(
    (Join-Path $PSScriptRoot "package_external_tools.py")
)
if ($RequireDelivery) {
    $arguments += "--require-delivery"
}
& py -3 @arguments
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
