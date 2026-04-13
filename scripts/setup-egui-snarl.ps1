# Setup script for patched egui-snarl
# This clones egui-snarl and patches it for scroll-to-zoom support

$snarlDir = Join-Path $PSScriptRoot "..\libs\egui-snarl"
$patchFile = Join-Path $PSScriptRoot "egui-snarl-scroll-zoom.patch"
$uiRsPath = Join-Path $snarlDir "src\ui.rs"
$snarlRevision = "bbed414980a14f949fe1bc137ced8bc5706a93c2"

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
