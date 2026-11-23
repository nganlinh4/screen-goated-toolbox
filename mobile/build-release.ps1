# SGT Mobile Release Build Script
# Usage:
#   powershell -ExecutionPolicy Bypass -File mobile\build-release.ps1
#   powershell -ExecutionPolicy Bypass -File mobile\build-release.ps1 -IncludeAab
#
param(
    [switch]$IncludeAab
)

# Builds:
#   - Full flavor APK  (direct distribution)
#   - Optional Play flavor AAB  (store-distribution artifact)
# Phone Control and its user-granted overlay capability are identical in both artifacts;
# this script changes packaging and delivery, not the feature's catalog or authority.

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path $PSScriptRoot -Parent
$mobileDir = $PSScriptRoot

# --- Environment ---
# Toolchain locations are machine state: JAVA_HOME comes from the environment and the
# SDK from ANDROID_HOME, ANDROID_SDK_ROOT or the gitignored local.properties.
. (Join-Path $PSScriptRoot "scripts\android-toolchain.ps1")
try {
    $env:JAVA_HOME = Resolve-ReleaseJavaHome
    $sdkRoot = Resolve-AndroidSdkRoot -MobileDir $mobileDir
}
catch {
    Write-Host "ERROR: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
$env:ANDROID_HOME = $sdkRoot
$env:ANDROID_SDK_ROOT = $sdkRoot

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

$artifactBaseName = "ScreenGoatedToolbox_v$version"

$outputApkName = "$artifactBaseName.apk"
$outputApkPath = "$targetDir\$outputApkName"
$outputAabName = "$artifactBaseName.aab"
$outputAabPath = "$targetDir\$outputAabName"

Write-Host ""
Write-Host "=== Building SGT Mobile v$version ===" -ForegroundColor Cyan

# --- Check keystore ---
$keystore = "$mobileDir\release.keystore"
if (-not (Test-Path $keystore)) {
    Write-Host "ERROR: release.keystore not found at $keystore" -ForegroundColor Red
    Write-Host "Generate one with:" -ForegroundColor Yellow
    Write-Host "  keytool -genkeypair -v -keystore mobile\release.keystore -alias sgt-release -keyalg RSA -keysize 2048 -validity 10000" -ForegroundColor Gray
    exit 1
}
# Gradle reads these (or sgt.release.* in ~/.gradle/gradle.properties); never print them.
$gradleProps = Join-Path $env:USERPROFILE ".gradle\gradle.properties"
$propsHaveSecrets = (Test-Path $gradleProps) -and
    (Select-String -Path $gradleProps -Pattern '^sgt\.release\.storePassword=' -Quiet) -and
    (Select-String -Path $gradleProps -Pattern '^sgt\.release\.keyPassword=' -Quiet)
if (-not $propsHaveSecrets -and (-not $env:SGT_RELEASE_STORE_PASSWORD -or -not $env:SGT_RELEASE_KEY_PASSWORD)) {
    Write-Host "ERROR: release signing passwords are not set." -ForegroundColor Red
    Write-Host "Set SGT_RELEASE_STORE_PASSWORD and SGT_RELEASE_KEY_PASSWORD, or add" -ForegroundColor Yellow
    Write-Host "sgt.release.storePassword / sgt.release.keyPassword to $gradleProps." -ForegroundColor Yellow
    exit 1
}

# --- Build release artifacts ---
if ($IncludeAab) {
    Write-Host "Building full release APK + play release AAB..." -ForegroundColor Gray
}
else {
    Write-Host "Building full release APK..." -ForegroundColor Gray
}

Push-Location $mobileDir
try {
    $gradleTasks = @(
        ":androidApp:assembleFullRelease"
        "-x", "lintVitalAnalyzeFullRelease"
        "-x", "lintVitalReportFullRelease"
        "-x", "lintVitalFullRelease"
    )

    if ($IncludeAab) {
        # The compliance gate builds the Play bundle and then checks its base module,
        # so a store artifact is never copied out without passing it.
        $gradleTasks += @(
            ":androidApp:verifyPlayReleaseCompliance"
            "-x", "lintVitalAnalyzePlayRelease"
            "-x", "lintVitalReportPlayRelease"
            "-x", "lintVitalPlayRelease"
        )
    }

    & .\gradlew.bat @gradleTasks --console=plain
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

$aabSize = $null
if ($IncludeAab) {
    # --- Copy AAB ---
    $builtAab = "$mobileDir\androidApp\build\outputs\bundle\playRelease\androidApp-play-release.aab"
    if (Test-Path $builtAab) {
        if (Test-Path $outputAabPath) { Remove-Item $outputAabPath }
        Copy-Item $builtAab $outputAabPath
        $aabSize = [Math]::Round((Get-Item $outputAabPath).Length / 1MB, 2)
    } else {
        Write-Host "  -> WARNING: play release AAB not found" -ForegroundColor Yellow
    }
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
