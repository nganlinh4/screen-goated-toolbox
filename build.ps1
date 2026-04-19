param(
    [ValidateSet("x64", "arm64", "all")]
    [string]$Arch = "x64"
)

# Re-patch egui-snarl to ensure custom scroll-to-zoom is applied
Write-Host "Setting up patched egui-snarl..." -ForegroundColor Cyan
$snarlDir = Join-Path $PSScriptRoot "libs\egui-snarl"
if (Test-Path $snarlDir) {
    Remove-Item $snarlDir -Recurse -Force
}
& (Join-Path $PSScriptRoot "scripts\setup-egui-snarl.ps1")

# --- Build PromptDJ Frontend ---
Write-Host "Building PromptDJ Frontend..." -ForegroundColor Cyan
$pdjDir = Join-Path $PSScriptRoot "promptdj-midi"
$pdjDist = Join-Path $pdjDir "dist"
$pdjTargetDist = Join-Path $PSScriptRoot "src\overlay\prompt_dj\dist"

Push-Location $pdjDir
try {
    if (-not (Test-Path "node_modules")) {
        npm install
        if ($LASTEXITCODE -ne 0) {
            Write-Host "FAILED: PromptDJ npm install failed." -ForegroundColor Red
            exit 1
        }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: PromptDJ build failed." -ForegroundColor Red
        exit 1
    }
}
finally {
    Pop-Location
}

if (Test-Path $pdjDist) {
    if (-not (Test-Path $pdjTargetDist)) {
        New-Item -ItemType Directory -Path $pdjTargetDist -Force | Out-Null
    }
    Copy-Item -Path "$pdjDist\*" -Destination $pdjTargetDist -Recurse -Force
    Write-Host "PromptDJ assets synchronized." -ForegroundColor Green
}
else {
    Write-Host "FAILED: PromptDJ build did not produce dist folder." -ForegroundColor Red
    exit 1
}

# --- Build Translation Gummy Frontend ---
Write-Host "Building Translation Gummy Frontend..." -ForegroundColor Cyan
$brDir = Join-Path $PSScriptRoot "translation-gummy-ui"
$brDist = Join-Path $brDir "dist"
$brTargetDist = Join-Path $PSScriptRoot "src\overlay\translation_gummy\dist"

Push-Location $brDir
try {
    if (-not (Test-Path "node_modules") -or -not (Test-Path "node_modules\\.bin\\vite.cmd")) {
        npm install
        if ($LASTEXITCODE -ne 0) {
            Write-Host "FAILED: Translation Gummy npm install failed." -ForegroundColor Red
            exit 1
        }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: Translation Gummy build failed." -ForegroundColor Red
        exit 1
    }
}
finally {
    Pop-Location
}

if (Test-Path $brDist) {
    if (-not (Test-Path $brTargetDist)) {
        New-Item -ItemType Directory -Path $brTargetDist -Force | Out-Null
    }
    Copy-Item -Path "$brDist\*" -Destination $brTargetDist -Recurse -Force
    Write-Host "Translation Gummy assets synchronized." -ForegroundColor Green
}
else {
    Write-Host "FAILED: Translation Gummy build did not produce dist folder." -ForegroundColor Red
    exit 1
}

# --- Build Screen Record Frontend ---
Write-Host "Building Screen Record Frontend..." -ForegroundColor Cyan
$srDir = Join-Path $PSScriptRoot "screen-record"
$srDist = Join-Path $srDir "dist"
$srTargetDist = Join-Path $PSScriptRoot "src\overlay\screen_record\dist"

Push-Location $srDir
try {
    if (-not (Test-Path "node_modules")) {
        npm install
        if ($LASTEXITCODE -ne 0) {
            Write-Host "FAILED: Screen Record npm install failed." -ForegroundColor Red
            exit 1
        }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: Screen Record build failed." -ForegroundColor Red
        exit 1
    }
}
finally {
    Pop-Location
}

if (Test-Path $srDist) {
    if (-not (Test-Path $srTargetDist)) {
        New-Item -ItemType Directory -Path $srTargetDist -Force | Out-Null
    }
    Copy-Item -Path "$srDist\*" -Destination $srTargetDist -Recurse -Force
    Write-Host "Screen Record assets synchronized." -ForegroundColor Green
}
else {
    Write-Host "FAILED: Screen Record build did not produce dist folder." -ForegroundColor Red
    exit 1
}

# --- Continue Main Build ---
# Extract version from Cargo.toml
$cargoContent = Get-Content "Cargo.toml" -Raw
if ($cargoContent -match 'version\s*=\s*"([^"]+)"') {
    $version = $matches[1]
}
else {
    Write-Host "Failed to extract version from Cargo.toml" -ForegroundColor Red
    exit 1
}

$targetMap = @{
    "x64" = "x86_64-pc-windows-msvc"
    "arm64" = "aarch64-pc-windows-msvc"
}

$selectedArchs = if ($Arch -eq "all") { @("x64", "arm64") } else { @($Arch) }
$builtArtifacts = @()

# =============================================================================
# Build Release version (LTO optimized + stripped)
# =============================================================================
foreach ($archName in $selectedArchs) {
    $targetTriple = $targetMap[$archName]
    $targetDir = "target/$targetTriple/release"
    $exePathRelease = Join-Path $targetDir "screen-goated-toolbox.exe"
    $outputExeName = if ($archName -eq "x64") {
        "ScreenGoatedToolbox_v$version.exe"
    } else {
        "ScreenGoatedToolbox_v$version-$archName.exe"
    }
    $outputPath = Join-Path $targetDir $outputExeName
    $legacyX64Path = if ($archName -eq "x64") {
        Join-Path $targetDir "ScreenGoatedToolbox_v$version-x64.exe"
    } else {
        $null
    }

    Write-Host ""
    Write-Host "=== Building ScreenGoatedToolbox v$version ($archName) ===" -ForegroundColor Cyan
    Write-Host "Using 'release' profile (LTO + stripped)..." -ForegroundColor Gray
    cargo build --release --target $targetTriple

    if (Test-Path $exePathRelease) {
        if ($legacyX64Path -and (Test-Path $legacyX64Path)) {
            Remove-Item $legacyX64Path
        }
        if (Test-Path $outputPath) {
            Remove-Item $outputPath
        }
        Copy-Item $exePathRelease $outputPath
        $size = (Get-Item $outputPath).Length / 1MB
        $builtArtifacts += [PSCustomObject]@{
            Name = $outputExeName
            Size = [Math]::Round($size, 2)
            Target = $targetTriple
        }
        Write-Host "  -> Created: $outputExeName ($([Math]::Round($size, 2)) MB)" -ForegroundColor Green
    }
    else {
        Write-Host "  -> FAILED: release build did not produce exe for $targetTriple" -ForegroundColor Red
        exit 1
    }
}

# =============================================================================
# SUMMARY
# =============================================================================
Write-Host ""
Write-Host "=======================================" -ForegroundColor White
Write-Host "         BUILD COMPLETE v$version" -ForegroundColor White
Write-Host "=======================================" -ForegroundColor White
Write-Host ""
foreach ($artifact in $builtArtifacts) {
    Write-Host "  $($artifact.Name)" -ForegroundColor Green
    Write-Host "  Target: $($artifact.Target)" -ForegroundColor Gray
    Write-Host "  Size: $($artifact.Size) MB" -ForegroundColor Gray
    Write-Host ""
}
