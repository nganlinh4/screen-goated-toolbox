# SGT Mobile Release Build Script
# Usage: powershell -ExecutionPolicy Bypass -File mobile\build-release.ps1

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

$outputApkName = "ScreenGoatedToolbox_v$version.apk"
$outputPath = "$repoRoot\target\release\$outputApkName"

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

# --- Build release APK ---
Write-Host "Building release APK (minified + signed)..." -ForegroundColor Gray
Push-Location $mobileDir
try {
    .\gradlew.bat :androidApp:assembleFullRelease `
        -x lintVitalAnalyzeFullRelease `
        -x lintVitalReportFullRelease `
        -x lintVitalFullRelease `
        --console=plain
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  -> FAILED: Gradle build failed" -ForegroundColor Red
        exit 1
    }
}
finally {
    Pop-Location
}

# --- Locate and copy APK ---
$builtApk = "$mobileDir\androidApp\build\outputs\apk\full\release\androidApp-full-release.apk"
if (-not (Test-Path $builtApk)) {
    Write-Host "  -> FAILED: release APK not found at $builtApk" -ForegroundColor Red
    Write-Host "  Check if signing is configured correctly." -ForegroundColor Yellow
    exit 1
}

$targetDir = "$repoRoot\target\release"
if (-not (Test-Path $targetDir)) {
    New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
}
if (Test-Path $outputPath) {
    Remove-Item $outputPath
}
Copy-Item $builtApk $outputPath

$size = (Get-Item $outputPath).Length / 1MB

# --- Summary ---
Write-Host ""
Write-Host "=======================================" -ForegroundColor White
Write-Host "      MOBILE BUILD COMPLETE v$version" -ForegroundColor White
Write-Host "=======================================" -ForegroundColor White
Write-Host ""
Write-Host "  $outputApkName ($([Math]::Round($size, 2)) MB)" -ForegroundColor Green
Write-Host "  -> $outputPath" -ForegroundColor Gray
Write-Host ""
