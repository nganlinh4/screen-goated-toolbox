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
    Write-Host "Screen Record frontend is ready for external packaging." -ForegroundColor Green
}
else {
    Write-Host "FAILED: Screen Record build did not produce dist folder." -ForegroundColor Red
    exit 1
}

# --- Build 3D Generator Frontend ---
Write-Host "Building 3D Generator Frontend..." -ForegroundColor Cyan
$gen3dDir = Join-Path $PSScriptRoot "3d-generator-ui"
$gen3dTargetDist = Join-Path $PSScriptRoot "src\overlay\three_d_generator\dist"

Push-Location $gen3dDir
try {
    if (-not (Test-Path "node_modules") -or -not (Test-Path "node_modules\\.bin\\vite.cmd")) {
        npm install
        if ($LASTEXITCODE -ne 0) {
            Write-Host "FAILED: 3D Generator npm install failed." -ForegroundColor Red
            exit 1
        }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: 3D Generator build failed." -ForegroundColor Red
        exit 1
    }
}
finally {
    Pop-Location
}

if (Test-Path $gen3dTargetDist) {
    Write-Host "3D Generator assets synchronized." -ForegroundColor Green
}
else {
    Write-Host "FAILED: 3D Generator build did not produce dist folder." -ForegroundColor Red
    exit 1
}

# --- Build TTS Playground Frontend ---
Write-Host "Building TTS Playground Frontend..." -ForegroundColor Cyan
$ttsDir = Join-Path $PSScriptRoot "tts-playground-ui"
$ttsDist = Join-Path $ttsDir "dist"
$ttsTargetDist = Join-Path $PSScriptRoot "src\overlay\tts_playground\dist"

Push-Location $ttsDir
try {
    if (-not (Test-Path "node_modules")) {
        npm install
        if ($LASTEXITCODE -ne 0) {
            Write-Host "FAILED: TTS Playground npm install failed." -ForegroundColor Red
            exit 1
        }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: TTS Playground build failed." -ForegroundColor Red
        exit 1
    }
}
finally {
    Pop-Location
}

if (Test-Path $ttsDist) {
    if (-not (Test-Path $ttsTargetDist)) {
        New-Item -ItemType Directory -Path $ttsTargetDist -Force | Out-Null
    }
    Copy-Item -Path "$ttsDist\*" -Destination $ttsTargetDist -Recurse -Force
    Write-Host "TTS Playground assets synchronized." -ForegroundColor Green
}
else {
    Write-Host "FAILED: TTS Playground build did not produce dist folder." -ForegroundColor Red
    exit 1
}

# The private creation-runtime build produces a read-back-verified combined
# manifest shared by Windows and both Android flavors. Never compile a release
# host that silently omits the approved image-to-3D runtime.
$creationRuntimeDelivery = Join-Path $PSScriptRoot "local-runtime-bundles\sgt_creation_runtime\sgt_creation_runtime.delivery.json"
if (-not (Test-Path -LiteralPath $creationRuntimeDelivery -PathType Leaf)) {
    Write-Host "FAILED: verified creation-runtime delivery is missing. Rebuild and verify the private Windows/Android runtime release first." -ForegroundColor Red
    exit 1
}
Write-Host "Reading back creation-runtime delivery..." -ForegroundColor Cyan
& py -3 (Join-Path $PSScriptRoot "scripts\verify_creation_runtime_release.py") --manifest $creationRuntimeDelivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: creation-runtime delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$env:SGT_CREATION_RUNTIME_DELIVERY_MANIFEST = $creationRuntimeDelivery

# Build deterministic optional frontend packs and require read-back-verified delivery metadata.
# This prevents a signed host from referencing an asset that has not reached the immutable release.
Write-Host "Packaging optional frontend bundles..." -ForegroundColor Cyan
& py -3 (Join-Path $PSScriptRoot "scripts\package_web_assets.py") --require-delivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: optional frontend delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$env:SGT_WEB_ASSET_DELIVERY_MANIFEST = Join-Path $PSScriptRoot "local-runtime-bundles\sgt_web_assets\sgt_web_assets.delivery.json"

# Pin external executables and the signed WebView2 bootstrapper to bytes read
# back from immutable release locations. None of these payloads enters the host.
Write-Host "Verifying Windows external-tool delivery..." -ForegroundColor Cyan
& (Join-Path $PSScriptRoot "scripts\build-external-tool-packs.ps1") -RequireDelivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: Windows external-tool delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$env:SGT_EXTERNAL_TOOL_DELIVERY_MANIFEST = Join-Path $PSScriptRoot "local-runtime-bundles\sgt_external_tools\sgt_external_tools.delivery.json"

# Require the canonical Windows model package inventory and prove that its
# deterministic tracked delivery plus every local package entry still match.
$windowsModelDelivery = Join-Path $PSScriptRoot "local-runtime-bundles\sgt_windows_models\sgt_windows_model_packages.json"
if (-not (Test-Path -LiteralPath $windowsModelDelivery -PathType Leaf)) {
    Write-Host "FAILED: canonical Windows model delivery manifest is missing." -ForegroundColor Red
    exit 1
}
$env:SGT_WINDOWS_MODEL_DELIVERY_MANIFEST = $windowsModelDelivery
Write-Host "Verifying Windows model delivery..." -ForegroundColor Cyan
& py -3 (Join-Path $PSScriptRoot "scripts\verify_windows_model_release.py") `
    --package-manifest $env:SGT_WINDOWS_MODEL_DELIVERY_MANIFEST `
    --delivery-manifest (Join-Path $PSScriptRoot "model-delivery\windows-v1.json")
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: Windows model delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}

# Pin the independently removable VC support component to bytes read back from the
# append-only runtime-bundles release. No VC payload is embedded in the host.
Write-Host "Packaging Windows VC runtime support..." -ForegroundColor Cyan
& py -3 (Join-Path $PSScriptRoot "scripts\package_vc_runtime.py") --require-delivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: VC runtime delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$env:SGT_VC_RUNTIME_DELIVERY_MANIFEST = Join-Path $PSScriptRoot "local-runtime-bundles\sgt_vc_runtime\sgt_vc_runtime.delivery.json"

# Reproduce the split Qwen3 CUDA packs and require delivery metadata generated by
# hashing the published assets after upload. This is intentionally fail-closed.
Write-Host "Verifying Qwen3 CUDA runtime delivery..." -ForegroundColor Cyan
& py -3 (Join-Path $PSScriptRoot "scripts\package_qwen3_runtime.py") --require-delivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: Qwen3 CUDA runtime delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$env:SGT_QWEN3_RUNTIME_DELIVERY_MANIFEST = Join-Path $PSScriptRoot "local-runtime-bundles\sgt_qwen3_runtime\sgt_qwen3_runtime.delivery.json"

# Local ASR packs are built explicitly because they include a separate release
# worker. Canonical host builds consume only the read-back-verified manifest.
$localAsrDelivery = Join-Path $PSScriptRoot "local-runtime-bundles\sgt_local_asr\sgt_local_asr.delivery.json"
if (-not (Test-Path -LiteralPath $localAsrDelivery -PathType Leaf)) {
    Write-Host "FAILED: verified local ASR delivery is missing. Build, upload, and run scripts\verify_local_asr_release.py first." -ForegroundColor Red
    exit 1
}
$env:SGT_LOCAL_ASR_DELIVERY_MANIFEST = $localAsrDelivery

# Build the standalone recorder worker and reproduce both recorder packages.
# The host build is blocked unless their uploaded bytes were read back and
# recorded in the verified delivery manifest.
Write-Host "Verifying Screen Recorder component delivery..." -ForegroundColor Cyan
& (Join-Path $PSScriptRoot "scripts\build-recorder-component-packs.ps1") -RequireDelivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: Screen Recorder component delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$env:SGT_RECORDER_DELIVERY_MANIFEST = Join-Path $PSScriptRoot "local-runtime-bundles\sgt_recorder\sgt_recorder.delivery.json"

# Build the standalone Computer Control cognition engine and require delivery
# metadata produced only after the immutable release asset was read back.
Write-Host "Verifying Computer Control engine delivery..." -ForegroundColor Cyan
& (Join-Path $PSScriptRoot "scripts\build-computer-control-engine-pack.ps1") -RequireDelivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: Computer Control engine delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$env:SGT_COMPUTER_CONTROL_DELIVERY_MANIFEST = Join-Path $PSScriptRoot "local-runtime-bundles\sgt_computer_control\sgt_computer_control.delivery.json"

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
}

$selectedArchs = @("x64")
$builtArtifacts = @()

# Keep build-machine paths out of panic locations and release debug metadata. The encoded form
# preserves Windows paths containing spaces and replaces the target rustflags from .cargo/config,
# so the static CRT flag is repeated here intentionally.
$rustFlagSeparator = [char]0x1f
$workspaceRoot = [IO.Path]::GetFullPath($PSScriptRoot).TrimEnd('\')
$releaseRustFlags = @(
    "-C",
    "target-feature=+crt-static",
    "--remap-path-prefix=$workspaceRoot=/sgt"
)
$cargoHome = if ($env:CARGO_HOME) {
    [IO.Path]::GetFullPath($env:CARGO_HOME).TrimEnd('\')
}
else {
    [IO.Path]::GetFullPath((Join-Path $HOME ".cargo")).TrimEnd('\')
}
$releaseRustFlags += "--remap-path-prefix=$cargoHome=/cargo"

$userProfile = [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)
$privateBuildPaths = @($workspaceRoot, $cargoHome)
if (-not [string]::IsNullOrWhiteSpace($userProfile)) {
    $userProfile = [IO.Path]::GetFullPath($userProfile).TrimEnd('\')
    $releaseRustFlags += "--remap-path-prefix=$userProfile=/build-user"
    $privateBuildPaths += $userProfile
}

$previousEncodedRustFlags = [Environment]::GetEnvironmentVariable(
    "CARGO_ENCODED_RUSTFLAGS",
    [EnvironmentVariableTarget]::Process
)
if (-not [string]::IsNullOrEmpty($previousEncodedRustFlags)) {
    $releaseRustFlags += $previousEncodedRustFlags.Split($rustFlagSeparator)
}
$encodedReleaseRustFlags = $releaseRustFlags -join $rustFlagSeparator

# Native dependencies can embed absolute __FILE__ paths independently of rustc.
$nativePathMappings = @(
    [PSCustomObject]@{ Source = $workspaceRoot; Destination = "/sgt" },
    [PSCustomObject]@{ Source = $cargoHome; Destination = "/cargo" }
)
if (-not [string]::IsNullOrWhiteSpace($userProfile)) {
    $nativePathMappings += [PSCustomObject]@{ Source = $userProfile; Destination = "/build-user" }
}
$previousCFlags = [Environment]::GetEnvironmentVariable("CFLAGS", [EnvironmentVariableTarget]::Process)
$previousCxxFlags = [Environment]::GetEnvironmentVariable("CXXFLAGS", [EnvironmentVariableTarget]::Process)
$previousCmakeCFlags = [Environment]::GetEnvironmentVariable("CMAKE_C_FLAGS", [EnvironmentVariableTarget]::Process)
$previousCmakeCxxFlags = [Environment]::GetEnvironmentVariable("CMAKE_CXX_FLAGS", [EnvironmentVariableTarget]::Process)

function Join-NativeReleaseFlags {
    param(
        [string]$Existing,
        [string[]]$Additional
    )
    return (@($Existing, ($Additional -join " ")) |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) -join " "
}

function Get-NativeReleaseFlags {
    $flags = @($nativePathMappings | ForEach-Object {
        "/pathmap:$($_.Source)=$($_.Destination)"
    })
    $flags += @($nativePathMappings | ForEach-Object {
        "/d1trimfile:$($_.Source)\"
    })
    return $flags
}

function Assert-ReleaseBinaryPrivacy {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BinaryPath,
        [Parameter(Mandatory = $true)]
        [string[]]$PrivatePrefixes
    )

    # Rust source locations are UTF-8 in the executable. Decode as ASCII so arbitrary binary
    # bytes cannot prevent a literal private path from being found.
    $binaryText = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($BinaryPath))
    foreach ($prefix in $PrivatePrefixes) {
        foreach ($candidate in @($prefix, $prefix.Replace('\', '/'))) {
            if ($binaryText.IndexOf($candidate, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                throw "Release artifact contains a private build path: $candidate"
            }
        }
    }
}

# =============================================================================
# Build Release version (LTO optimized + stripped)
# =============================================================================
foreach ($archName in $selectedArchs) {
    $targetTriple = $targetMap[$archName]
    $targetDir = "target/$targetTriple/release"
    $exePathRelease = Join-Path $targetDir "screen-goated-toolbox.exe"
    $outputExeName = "ScreenGoatedToolbox_v$version.exe"
    $outputPath = Join-Path $targetDir $outputExeName
    $legacyX64Path = Join-Path $targetDir "ScreenGoatedToolbox_v$version-x64.exe"

    Write-Host ""
    Write-Host "=== Building ScreenGoatedToolbox v$version ($archName) ===" -ForegroundColor Cyan
    Write-Host "Using 'release' profile (LTO + stripped)..." -ForegroundColor Gray
    Write-Host "Remapping private build paths in release metadata..." -ForegroundColor Gray
    $nativeReleaseFlags = Get-NativeReleaseFlags
    $env:CARGO_ENCODED_RUSTFLAGS = $encodedReleaseRustFlags
    $env:CFLAGS = Join-NativeReleaseFlags $previousCFlags $nativeReleaseFlags
    $env:CXXFLAGS = Join-NativeReleaseFlags $previousCxxFlags $nativeReleaseFlags
    $env:CMAKE_C_FLAGS = Join-NativeReleaseFlags $previousCmakeCFlags $nativeReleaseFlags
    $env:CMAKE_CXX_FLAGS = Join-NativeReleaseFlags $previousCmakeCxxFlags $nativeReleaseFlags
    $cargoExitCode = 0
    try {
        cargo build --release --target $targetTriple
        $cargoExitCode = $LASTEXITCODE
    }
    finally {
        if ($null -eq $previousEncodedRustFlags) {
            Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
        }
        else {
            $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedRustFlags
        }
        if ($null -eq $previousCFlags) {
            Remove-Item Env:CFLAGS -ErrorAction SilentlyContinue
        }
        else {
            $env:CFLAGS = $previousCFlags
        }
        if ($null -eq $previousCxxFlags) {
            Remove-Item Env:CXXFLAGS -ErrorAction SilentlyContinue
        }
        else {
            $env:CXXFLAGS = $previousCxxFlags
        }
        if ($null -eq $previousCmakeCFlags) {
            Remove-Item Env:CMAKE_C_FLAGS -ErrorAction SilentlyContinue
        }
        else {
            $env:CMAKE_C_FLAGS = $previousCmakeCFlags
        }
        if ($null -eq $previousCmakeCxxFlags) {
            Remove-Item Env:CMAKE_CXX_FLAGS -ErrorAction SilentlyContinue
        }
        else {
            $env:CMAKE_CXX_FLAGS = $previousCmakeCxxFlags
        }
    }
    if ($cargoExitCode -ne 0) {
        Write-Host "  -> FAILED: cargo build exited with code $cargoExitCode" -ForegroundColor Red
        exit $cargoExitCode
    }

    if (Test-Path $exePathRelease) {
        if ($legacyX64Path -and (Test-Path $legacyX64Path)) {
            Remove-Item $legacyX64Path
        }
        if (Test-Path $outputPath) {
            Remove-Item $outputPath
        }
        Copy-Item $exePathRelease $outputPath
        Assert-ReleaseBinaryPrivacy -BinaryPath $outputPath -PrivatePrefixes $privateBuildPaths
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
