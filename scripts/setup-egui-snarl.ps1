# Setup script for patched egui-snarl
# Clones egui-snarl, checks out the pinned revision, and applies the patch
# (scroll-to-zoom + the Material node-collapse chevron) from
# egui-snarl-scroll-zoom.patch. All custom snarl changes live in that patch.

$snarlDir = Join-Path $PSScriptRoot "..\libs\egui-snarl"
$patchFile = Join-Path $PSScriptRoot "egui-snarl-scroll-zoom.patch"
$uiRsPath = Join-Path $snarlDir "src\ui.rs"
# Latest `main` (egui 0.34). The previous pin (bbed414) was stale and used egui
# 0.33, which mismatched the app's eframe/egui 0.34 and broke type unification.
$snarlRevision = "5bdc34e4ebdb9d7a0968f21564dce51a1a027ee8"

# Clone egui-snarl if needed
if (-not (Test-Path $snarlDir)) {
    Write-Host "Cloning egui-snarl..."
    git clone --depth 20 https://github.com/zakarumych/egui-snarl.git $snarlDir
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to clone egui-snarl."
        exit 1
    }
}

Write-Host "Checking out egui-snarl revision $snarlRevision..."
git -C $snarlDir checkout --force $snarlRevision
if ($LASTEXITCODE -ne 0) {
    Write-Error "Failed to checkout egui-snarl revision $snarlRevision."
    exit 1
}

if (-not (Test-Path $uiRsPath)) {
    Write-Error "Failed to locate egui-snarl/src/ui.rs at $uiRsPath"
    exit 1
}

# If already patched, do not apply again.
if (Select-String -Path $uiRsPath -Pattern "CUSTOM SCROLL-TO-ZOOM" -Quiet) {
    Write-Host "egui-snarl already patched at $snarlDir"
    exit 0
}

if (-not (Test-Path $patchFile)) {
    Write-Error "Missing patch file: $patchFile"
    exit 1
}

Write-Host "Applying scroll-to-zoom patch..."
git -C $snarlDir apply --whitespace=nowarn $patchFile
if ($LASTEXITCODE -ne 0) {
    Write-Error "Failed to apply egui-snarl scroll-to-zoom patch."
    exit 1
}

Write-Host "Patch applied successfully!"
Write-Host "egui-snarl is ready at: $snarlDir"
