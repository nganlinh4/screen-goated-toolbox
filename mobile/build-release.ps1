# SGT Mobile Release Build Script
# Usage: powershell -ExecutionPolicy Bypass -File mobile\build-release.ps1
#
# Builds both:
#   - Full flavor APK  (direct distribution, with overlay support)
#   - Play flavor AAB  (Google Play Store upload)

$ErrorActionPreference = "Stop"

# --- Environment ---
$env:JAVA_HOME = 'C:\Users\user\scoop\apps\temurin17-jdk\17.0.18-8'
$env:ANDROID_HOME = 'C:\Users\user\android-sdk'
$env:ANDROID_SDK_ROOT = 'C:\Users\user\android-sdk'

$repoRoot = Split-Path $PSScriptRoot -Parent
$mobileDir = $PSScriptRoot

# --- Extract version from Cargo.toml ---
$cargoContent = Get-Content "$repoRoot\Cargo.toml" -Raw
if ($cargoContent -match 'version\s*=\s*"([^"]+)"') {
    $version = $matches[1]
}
else {
    Write-Host "Failed to extract version from Cargo.toml" -ForegroundColor Red
    exit 1
}

$targetDir = "$repoRoot\target\release"
if (-not (Test-Path $targetDir)) {
    New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
}

$outputApkName = "ScreenGoatedToolbox_v$version.apk"
$outputApkPath = "$targetDir\$outputApkName"
$outputAabName = "ScreenGoatedToolbox_v$version.aab"
$outputAabPath = "$targetDir\$outputAabName"

Write-Host ""
Write-Host "=== Building SGT Mobile v$version ===" -ForegroundColor Cyan

# --- Check keystore ---
$keystore = "$mobileDir\release.keystore"
if (-not (Test-Path $keystore)) {
    Write-Host "ERROR: release.keystore not found at $keystore" -ForegroundColor Red
    Write-Host "Generate one with:" -ForegroundColor Yellow
    Write-Host "  keytool -genkeypair -v -keystore mobile\release.keystore -alias sgt-release -keyalg RSA -keysize 2048 -validity 10000 -storepass screengoated -keypass screengoated" -ForegroundColor Gray
    exit 1
}

# --- Build both APK (full) and AAB (play) ---
Write-Host "Building full release APK + play release AAB..." -ForegroundColor Gray
Push-Location $mobileDir
try {
    .\gradlew.bat `
        :androidApp:assembleFullRelease `
        :androidApp:bundlePlayRelease `
        -x lintVitalAnalyzeFullRelease -x lintVitalReportFullRelease -x lintVitalFullRelease `
        -x lintVitalAnalyzePlayRelease -x lintVitalReportPlayRelease -x lintVitalPlayRelease `
        --console=plain
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  -> FAILED: Gradle build failed" -ForegroundColor Red
        exit 1
    }
}
finally {
    Pop-Location
}

# --- Copy APK ---
$builtApk = "$mobileDir\androidApp\build\outputs\apk\full\release\androidApp-full-release.apk"
if (Test-Path $builtApk) {
    if (Test-Path $outputApkPath) { Remove-Item $outputApkPath }
    Copy-Item $builtApk $outputApkPath
    $apkSize = [Math]::Round((Get-Item $outputApkPath).Length / 1MB, 2)
} else {
    Write-Host "  -> WARNING: full release APK not found" -ForegroundColor Yellow
    $apkSize = $null
}

# --- Copy AAB ---
$builtAab = "$mobileDir\androidApp\build\outputs\bundle\playRelease\androidApp-play-release.aab"
if (Test-Path $builtAab) {
    if (Test-Path $outputAabPath) { Remove-Item $outputAabPath }
    Copy-Item $builtAab $outputAabPath
    $aabSize = [Math]::Round((Get-Item $outputAabPath).Length / 1MB, 2)
} else {
    Write-Host "  -> WARNING: play release AAB not found" -ForegroundColor Yellow
    $aabSize = $null
}

# --- Summary ---
Write-Host ""
Write-Host "=======================================" -ForegroundColor White
Write-Host "      MOBILE BUILD COMPLETE v$version" -ForegroundColor White
Write-Host "=======================================" -ForegroundColor White
Write-Host ""
if ($apkSize) {
    Write-Host "  APK: $outputApkName ($apkSize MB)" -ForegroundColor Green
    Write-Host "       -> $outputApkPath" -ForegroundColor Gray
}
if ($aabSize) {
    Write-Host "  AAB: $outputAabName ($aabSize MB)" -ForegroundColor Green
    Write-Host "       -> $outputAabPath" -ForegroundColor Gray
}
Write-Host ""
