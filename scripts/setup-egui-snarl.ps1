# Setup script for patched egui-snarl
# Clones egui-snarl, checks out the pinned revision, and applies the patch
# (scroll-to-zoom + the Material node-collapse chevron) from
# egui-snarl-scroll-zoom.patch. All custom snarl changes live in that patch.

$snarlDir = Join-Path $PSScriptRoot "..\libs\egui-snarl"
$patchFile = Join-Path $PSScriptRoot "egui-snarl-scroll-zoom.patch"
$fontPatchFile = Join-Path $PSScriptRoot "egui-snarl-no-default-fonts.patch"
$uiRsPath = Join-Path $snarlDir "src\ui.rs"
$snarlCargoPath = Join-Path $snarlDir "Cargo.toml"
$scaleDir = Join-Path $PSScriptRoot "..\libs\egui-scale"
$scalePatchFile = Join-Path $PSScriptRoot "egui-scale-no-default-fonts.patch"
# Latest `main` (egui 0.34). The previous pin (bbed414) was stale and used egui
# 0.33, which mismatched the app's eframe/egui 0.34 and broke type unification.
$snarlRevision = "5bdc34e4ebdb9d7a0968f21564dce51a1a027ee8"
$scaleRevision = "abb9b647cf9478c6de876a980e4355cdc2d141c8"

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

if (-not (Test-Path $patchFile)) {
    Write-Error "Missing patch file: $patchFile"
    exit 1
}

if (-not (Select-String -Path $uiRsPath -Pattern "CUSTOM SCROLL-TO-ZOOM" -Quiet)) {
    Write-Host "Applying scroll-to-zoom patch..."
    git -C $snarlDir apply --whitespace=nowarn $patchFile
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to apply egui-snarl scroll-to-zoom patch."
        exit 1
    }
}

if (-not (Select-String -Path $snarlCargoPath -Pattern 'egui = \{ version = "0.34", default-features = false \}' -Quiet)) {
    Write-Host "Disabling egui default fonts in egui-snarl..."
    git -C $snarlDir apply --whitespace=nowarn $fontPatchFile
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to apply the egui-snarl feature patch."
        exit 1
    }
}

if (-not (Test-Path $scaleDir)) {
    Write-Host "Cloning egui-scale..."
    git clone https://github.com/zakarumych/egui-scale.git $scaleDir
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to clone egui-scale."
        exit 1
    }
}

Write-Host "Checking out egui-scale revision $scaleRevision..."
git -C $scaleDir checkout --force $scaleRevision
if ($LASTEXITCODE -ne 0) {
    Write-Error "Failed to checkout egui-scale revision $scaleRevision."
    exit 1
}

$scaleCargoPath = Join-Path $scaleDir "Cargo.toml"
if (-not (Select-String -Path $scaleCargoPath -Pattern 'egui = \{ version = "0.34", default-features = false \}' -Quiet)) {
    git -C $scaleDir apply --whitespace=nowarn $scalePatchFile
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to apply the egui-scale feature patch."
        exit 1
    }
}

Write-Host "Patch applied successfully!"
Write-Host "egui-snarl is ready at: $snarlDir"
