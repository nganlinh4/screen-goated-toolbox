$ErrorActionPreference = "Stop"

$validator = Join-Path $PSScriptRoot "validate-egui-patches.ps1"
$setup = Join-Path $PSScriptRoot "setup-egui-snarl.ps1"
$repoRoot = Split-Path -Parent $PSScriptRoot
$contract = & (Join-Path $PSScriptRoot "egui-patch-contract.ps1")
$initialized = @(
    $contract.Dependencies | Where-Object {
        Test-Path -LiteralPath (Join-Path $repoRoot "$($_.RelativeDirectory)/.git")
    }
)

if ($initialized.Count -eq 0) {
    & $setup
}
elseif ($initialized.Count -ne $contract.Dependencies.Count) {
    throw "Patched egui dependency setup is incomplete; preserve any local work, then run scripts/setup-egui-snarl.ps1 explicitly"
}
else {
    # Never silently discard a direct dependency edit. Existing checkouts must match exactly or
    # the developer must move the change into a tracked patch before recreating them explicitly.
    & $validator
}
