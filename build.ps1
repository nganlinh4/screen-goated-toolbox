if ($env:SGT_COMPONENT_DELIVERY_CHANNEL -eq "staging" -or
    -not [string]::IsNullOrWhiteSpace($env:SGT_STAGING_DELIVERY_ROOT)) {
    throw "Release builds cannot use mutable staging component delivery. Start a clean shell."
}

# Validate the full dependency trees, not just marker lines. Invalid disposable checkouts are
# recreated exclusively from the pinned revisions and tracked patches.
& (Join-Path $PSScriptRoot "scripts\ensure-egui-dependencies.ps1")

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

# --- Build Image to SVG Frontend ---
Write-Host "Building Image to SVG Frontend..." -ForegroundColor Cyan
$svgDir = Join-Path $PSScriptRoot "image-to-svg-ui"
$svgTargetDist = Join-Path $PSScriptRoot "src\overlay\image_to_svg\dist"

Push-Location $svgDir
try {
    if (-not (Test-Path "node_modules") -or -not (Test-Path "node_modules\.bin\vite.cmd")) {
        npm install
        if ($LASTEXITCODE -ne 0) {
            Write-Host "FAILED: Image to SVG npm install failed." -ForegroundColor Red
            exit 1
        }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: Image to SVG build failed." -ForegroundColor Red
        exit 1
    }
}
finally {
    Pop-Location
}

if (Test-Path $svgTargetDist) {
    Write-Host "Image to SVG assets synchronized." -ForegroundColor Green
}
else {
    Write-Host "FAILED: Image to SVG build did not produce dist folder." -ForegroundColor Red
    exit 1
}

# --- Build Image Creator Frontend ---
Write-Host "Building Image Creator Frontend..." -ForegroundColor Cyan
$imageCreatorDir = Join-Path $PSScriptRoot "image-creator-ui"
$imageCreatorTargetDist = Join-Path $PSScriptRoot "src\overlay\image_creator\dist"

Push-Location $imageCreatorDir
try {
    if (-not (Test-Path "node_modules") -or -not (Test-Path "node_modules\.bin\vite.cmd")) {
        npm install
        if ($LASTEXITCODE -ne 0) {
            Write-Host "FAILED: Image Creator npm install failed." -ForegroundColor Red
            exit 1
        }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: Image Creator build failed." -ForegroundColor Red
        exit 1
    }
}
finally {
    Pop-Location
}

if (Test-Path $imageCreatorTargetDist) {
    Write-Host "Image Creator assets synchronized." -ForegroundColor Green
}
else {
    Write-Host "FAILED: Image Creator build did not produce dist folder." -ForegroundColor Red
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

function Assert-TrackedDelivery {
    param(
        [Parameter(Mandatory = $true)][string]$Generated,
        [Parameter(Mandatory = $true)][string]$Tracked
    )

    & py -3 (Join-Path $PSScriptRoot "scripts\verify_tracked_delivery.py") $Generated $Tracked
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: verified remote delivery does not match the tracked build contract." -ForegroundColor Red
        exit $LASTEXITCODE
    }
}

function Initialize-DeliveryOutput {
    param(
        [Parameter(Mandatory = $true)][string]$Directory,
        [Parameter(Mandatory = $true)][string]$Tracked,
        [Parameter(Mandatory = $true)][string]$FileName
    )

    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    Copy-Item -LiteralPath $Tracked -Destination (Join-Path $Directory $FileName) -Force
}

$trackedDeliveryRoot = Join-Path $PSScriptRoot "component-delivery\windows"
$developmentCache = if (-not [string]::IsNullOrWhiteSpace($env:SGT_DEV_CACHE_ROOT)) {
    [IO.Path]::GetFullPath($env:SGT_DEV_CACHE_ROOT)
}
else {
    Join-Path ([Environment]::GetFolderPath(
        [Environment+SpecialFolder]::LocalApplicationData
    )) "SGT-Development\cache"
}
$releasePackageRoot = Join-Path $developmentCache "packages\release"
$packageCargoTarget = Join-Path $developmentCache "cargo\package"
$env:SGT_DEV_CACHE_ROOT = $developmentCache
New-Item -ItemType Directory -Path $releasePackageRoot -Force | Out-Null
Write-Host "Optional-package workspace: $releasePackageRoot" -ForegroundColor DarkGray

# The tracked creation contract is shared by Windows and both Android flavors
# and is verified against the immutable release before the host build starts.
$creationRuntimeDelivery = Join-Path $PSScriptRoot "component-delivery\creation-runtime-v1.json"
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

# Build deterministic optional frontend packs and require read-back-verified delivery metadata.
# This prevents a signed host from referencing an asset that has not reached the immutable release.
Write-Host "Packaging optional frontend bundles..." -ForegroundColor Cyan
$webAssetOutput = Join-Path $releasePackageRoot "sgt_web_assets"
& py -3 (Join-Path $PSScriptRoot "scripts\package_web_assets.py") `
    --output-dir $webAssetOutput --require-delivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: optional frontend delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$webAssetDelivery = Join-Path $webAssetOutput "sgt_web_assets.delivery.json"
Assert-TrackedDelivery $webAssetDelivery (Join-Path $trackedDeliveryRoot "web-assets-v1.json")

# Pin external executables and the signed WebView2 bootstrapper to bytes read
# back from immutable release locations. None of these payloads enters the host.
Write-Host "Verifying Windows external-tool delivery..." -ForegroundColor Cyan
$externalToolOutput = Join-Path $releasePackageRoot "sgt_external_tools"
Initialize-DeliveryOutput $externalToolOutput `
    (Join-Path $trackedDeliveryRoot "external-tools-v1.json") `
    "sgt_external_tools.delivery.json"
& (Join-Path $PSScriptRoot "scripts\build-external-tool-packs.ps1") `
    -OutputDir $externalToolOutput -RequireDelivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: Windows external-tool delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$externalToolDelivery = Join-Path $externalToolOutput "sgt_external_tools.delivery.json"
Assert-TrackedDelivery $externalToolDelivery (Join-Path $trackedDeliveryRoot "external-tools-v1.json")

# Require the canonical Windows model package inventory and prove that its
# deterministic tracked delivery plus every local package entry still match.
$windowsModelDelivery = if (-not [string]::IsNullOrWhiteSpace(
    $env:SGT_WINDOWS_MODEL_PACKAGE_MANIFEST
)) {
    [IO.Path]::GetFullPath($env:SGT_WINDOWS_MODEL_PACKAGE_MANIFEST)
}
else {
    $cachedModels = Join-Path $releasePackageRoot `
        "sgt_windows_models\sgt_windows_model_packages.json"
    if (Test-Path -LiteralPath $cachedModels -PathType Leaf) {
        $cachedModels
    }
    else {
        Join-Path $PSScriptRoot `
            "local-runtime-bundles\sgt_windows_models\sgt_windows_model_packages.json"
    }
}
if (-not (Test-Path -LiteralPath $windowsModelDelivery -PathType Leaf)) {
    Write-Host "FAILED: canonical Windows model delivery manifest is missing." -ForegroundColor Red
    exit 1
}
Write-Host "Verifying Windows model delivery..." -ForegroundColor Cyan
& py -3 (Join-Path $PSScriptRoot "scripts\verify_windows_model_release.py") `
    --package-manifest $windowsModelDelivery `
    --delivery-manifest (Join-Path $PSScriptRoot "model-delivery\windows-v1.json")
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: Windows model delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}

# Pin the independently removable VC support component to bytes read back from the
# append-only runtime-bundles release. No VC payload is embedded in the host.
Write-Host "Packaging Windows VC runtime support..." -ForegroundColor Cyan
$vcRuntimeOutput = Join-Path $releasePackageRoot "sgt_vc_runtime"
Initialize-DeliveryOutput $vcRuntimeOutput `
    (Join-Path $trackedDeliveryRoot "vc-runtime-v1.json") `
    "sgt_vc_runtime.delivery.json"
& py -3 (Join-Path $PSScriptRoot "scripts\package_vc_runtime.py") `
    --output-dir $vcRuntimeOutput --require-delivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: VC runtime delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$vcRuntimeDelivery = Join-Path $vcRuntimeOutput "sgt_vc_runtime.delivery.json"
Assert-TrackedDelivery $vcRuntimeDelivery (Join-Path $trackedDeliveryRoot "vc-runtime-v1.json")

# Reproduce the split Qwen3 CUDA packs and require delivery metadata generated by
# hashing the published assets after upload. This is intentionally fail-closed.
Write-Host "Verifying Qwen3 CUDA runtime delivery..." -ForegroundColor Cyan
$qwenRuntimeOutput = Join-Path $releasePackageRoot "sgt_qwen3_runtime"
Initialize-DeliveryOutput $qwenRuntimeOutput `
    (Join-Path $trackedDeliveryRoot "qwen-runtime-v1.json") `
    "sgt_qwen3_runtime.delivery.json"
& py -3 (Join-Path $PSScriptRoot "scripts\package_qwen3_runtime.py") `
    --output-dir $qwenRuntimeOutput --require-delivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: Qwen3 CUDA runtime delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$qwenRuntimeDelivery = Join-Path $qwenRuntimeOutput "sgt_qwen3_runtime.delivery.json"
Assert-TrackedDelivery $qwenRuntimeDelivery (Join-Path $trackedDeliveryRoot "qwen-runtime-v1.json")

# Local ASR packs are built explicitly because they include a separate release
# worker. Canonical host builds consume only the read-back-verified manifest.
$localAsrOutput = Join-Path $releasePackageRoot "sgt_local_asr"
New-Item -ItemType Directory -Path $localAsrOutput -Force | Out-Null
$localAsrDelivery = Join-Path $localAsrOutput "sgt_local_asr.delivery.json"
$localAsrPackages = Join-Path $localAsrOutput "sgt_local_asr.packages.json"
if (-not (Test-Path -LiteralPath $localAsrPackages -PathType Leaf)) {
    $localAsrPackages = Join-Path $PSScriptRoot `
        "local-runtime-bundles\sgt_local_asr\sgt_local_asr.packages.json"
}
Write-Host "Reading back local ASR component delivery..." -ForegroundColor Cyan
& py -3 (Join-Path $PSScriptRoot "scripts\verify_local_asr_release.py") `
    --packages $localAsrPackages --output $localAsrDelivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: local ASR delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
Assert-TrackedDelivery $localAsrDelivery (Join-Path $trackedDeliveryRoot "local-asr-v1.json")

# Build the standalone recorder worker and reproduce both recorder packages.
# The host build is blocked unless their uploaded bytes were read back and
# recorded in the verified delivery manifest.
Write-Host "Verifying Screen Recorder component delivery..." -ForegroundColor Cyan
$recorderOutput = Join-Path $releasePackageRoot "sgt_recorder"
Initialize-DeliveryOutput $recorderOutput `
    (Join-Path $trackedDeliveryRoot "recorder-v1.json") `
    "sgt_recorder.delivery.json"
& (Join-Path $PSScriptRoot "scripts\build-recorder-component-packs.ps1") `
    -OutputDir $recorderOutput -CargoTargetDir $packageCargoTarget -RequireDelivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: Screen Recorder component delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$recorderDelivery = Join-Path $recorderOutput "sgt_recorder.delivery.json"
Assert-TrackedDelivery $recorderDelivery (Join-Path $trackedDeliveryRoot "recorder-v1.json")

# Build the standalone Computer Control cognition engine and require delivery
# metadata produced only after the immutable release asset was read back.
Write-Host "Verifying Computer Control engine delivery..." -ForegroundColor Cyan
$computerControlOutput = Join-Path $releasePackageRoot "sgt_computer_control"
Initialize-DeliveryOutput $computerControlOutput `
    (Join-Path $trackedDeliveryRoot "computer-control-v1.json") `
    "sgt_computer_control.delivery.json"
& (Join-Path $PSScriptRoot "scripts\build-computer-control-engine-pack.ps1") `
    -OutputDir $computerControlOutput -CargoTargetDir $packageCargoTarget -RequireDelivery
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAILED: Computer Control engine delivery is not release-ready." -ForegroundColor Red
    exit $LASTEXITCODE
}
$computerControlDelivery = Join-Path $computerControlOutput "sgt_computer_control.delivery.json"
Assert-TrackedDelivery $computerControlDelivery (Join-Path $trackedDeliveryRoot "computer-control-v1.json")

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
    "-C",
    "link-arg=/Brepro",
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
