[CmdletBinding()]
param([switch]$RequireDelivery)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$arguments = @((Join-Path $PSScriptRoot "package_qwen3_runtime.py"))
if ($RequireDelivery) {
    $arguments += "--require-delivery"
}

Push-Location $repoRoot
try {
    & py -3 @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Qwen3 runtime packaging failed"
    }
}
finally {
    Pop-Location
}
