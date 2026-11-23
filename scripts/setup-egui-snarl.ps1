# Recreate disposable egui dependency checkouts from pinned revisions and tracked patches.

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$contract = & (Join-Path $PSScriptRoot "egui-patch-contract.ps1")

. (Join-Path $PSScriptRoot "egui-pinned-checkout.ps1")

foreach ($dependency in $contract.Dependencies) {
    $directory = Join-Path $repoRoot $dependency.RelativeDirectory
    Initialize-PinnedCheckout `
        -Name $dependency.Name `
        -Directory $directory `
        -Repository $dependency.Repository `
        -Revision $dependency.Revision `
        -Shallow:$dependency.Shallow

    foreach ($relativePatch in $dependency.Patches) {
        $patchPath = Join-Path $repoRoot $relativePatch
        if (-not (Test-Path -LiteralPath $patchPath -PathType Leaf)) {
            throw "Missing tracked patch: $relativePatch"
        }
        Write-Host "Applying $relativePatch..."
        git -C $directory apply --whitespace=nowarn $patchPath
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to apply $relativePatch to $($dependency.Name)."
        }
    }
}

& (Join-Path $PSScriptRoot "setup-size-patched-crates.ps1")
& (Join-Path $PSScriptRoot "validate-egui-patches.ps1")
Write-Host "Patched egui dependencies are ready."
