# Re-patch egui-snarl to ensure custom scroll-to-zoom is applied
Write-Host "Setting up patched egui-snarl..." -ForegroundColor Cyan
$snarlDir = Join-Path $PSScriptRoot "libs\egui-snarl"
if (Test-Path $snarlDir) {
    Remove-Item $snarlDir -Recurse -Force
}
& (Join-Path $PSScriptRoot "scripts\setup-egui-snarl.ps1")

# Extract version from Cargo.toml
$cargoContent = Get-Content "Cargo.toml" -Raw
if ($cargoContent -match 'version\s*=\s*"([^"]+)"') {
    $version = $matches[1]
}
else {
    Write-Host "Failed to extract version from Cargo.toml" -ForegroundColor Red
    exit 1
}

$upxDir = "tools/upx"
$upxPath = "$upxDir/upx.exe"

# Download UPX if not present
if (-not (Test-Path $upxPath)) {
    Write-Host "Downloading UPX..." -ForegroundColor Cyan
    New-Item -ItemType Directory -Path $upxDir -Force | Out-Null
    
    $url = "https://github.com/upx/upx/releases/download/v5.0.2/upx-5.0.2-win64.zip"
    $zip = "$upxDir/upx.zip"
    
    Invoke-WebRequest -Uri $url -OutFile $zip
    Expand-Archive -Path $zip -DestinationPath $upxDir -Force
    Move-Item "$upxDir/upx-5.0.2-win64/upx.exe" $upxPath -Force
    Remove-Item "$upxDir/upx-5.0.2-win64" -Recurse
    Remove-Item $zip
    
    Write-Host "UPX downloaded" -ForegroundColor Green
}

# Output paths
$outputExeNamePacked = "ScreenGoatedToolbox_v$version.exe"
$outputExeNameNoPack = "ScreenGoatedToolbox_v${version}_nopack.exe"
$outputPathPacked = "target/release/$outputExeNamePacked"
$outputPathNoPack = "target/release-safe/$outputExeNameNoPack"
$exePathRelease = "target/release/screen-goated-toolbox.exe"
$exePathSafe = "target/release-safe/screen-goated-toolbox.exe"

# =============================================================================
# STEP 1: Build AV-SAFE version (with debug symbols, no stripping)
# =============================================================================
Write-Host ""
Write-Host "=== Building AV-SAFE version (v$version) ===" -ForegroundColor Cyan
Write-Host "Using 'release-safe' profile with debug symbols for better AV trust..." -ForegroundColor Gray
$env:RUSTFLAGS="--cfg nopack"
cargo build --profile release-safe
$env:RUSTFLAGS=""

if (Test-Path $exePathSafe) {
    if (Test-Path $outputPathNoPack) {
        Remove-Item $outputPathNoPack
    }
    Move-Item $exePathSafe $outputPathNoPack
    $sizeNoPack = (Get-Item $outputPathNoPack).Length / 1MB
    Write-Host "  -> Created: $outputExeNameNoPack ($([Math]::Round($sizeNoPack, 2)) MB)" -ForegroundColor Cyan
}
else {
    Write-Host "  -> FAILED: release-safe build did not produce exe" -ForegroundColor Red
}

# =============================================================================
# STEP 2: Build PACKED version (stripped + UPX compressed)
# =============================================================================
Write-Host ""
Write-Host "=== Building PACKED version (v$version) ===" -ForegroundColor Green
Write-Host "Using 'release' profile with UPX compression..." -ForegroundColor Gray
cargo build --release

if (Test-Path $exePathRelease) {
    Write-Host "Compressing with UPX (--ultra-brute --lzma)..." -ForegroundColor Green
    & $upxPath --ultra-brute --lzma $exePathRelease
    
    if (Test-Path $outputPathPacked) {
        Remove-Item $outputPathPacked
    }
    Move-Item $exePathRelease $outputPathPacked
    $sizePacked = (Get-Item $outputPathPacked).Length / 1MB
    Write-Host "  -> Created: $outputExeNamePacked ($([Math]::Round($sizePacked, 2)) MB)" -ForegroundColor Green
}
else {
    Write-Host "  -> FAILED: release build did not produce exe" -ForegroundColor Red
}

# =============================================================================
# SUMMARY
# =============================================================================
Write-Host ""
Write-Host "=======================================" -ForegroundColor White
Write-Host "         BUILD COMPLETE v$version" -ForegroundColor White
Write-Host "=======================================" -ForegroundColor White
Write-Host ""
if (Test-Path $outputPathPacked) {
    Write-Host "  [PACKED]   $outputExeNamePacked" -ForegroundColor Green
    Write-Host "             Size: $([Math]::Round($sizePacked, 2)) MB | UPX compressed" -ForegroundColor Gray
}
if (Test-Path $outputPathNoPack) {
    Write-Host ""
    Write-Host "  [AV-SAFE]  $outputExeNameNoPack" -ForegroundColor Cyan
    Write-Host "             Size: $([Math]::Round($sizeNoPack, 2)) MB | Has debug symbols, no UPX" -ForegroundColor Gray
}
Write-Host ""
Write-Host "TIP: Offer '_nopack' to users with Windows Defender issues." -ForegroundColor Yellow
Write-Host ""

