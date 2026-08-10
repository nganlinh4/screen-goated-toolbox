param(
    [Parameter(Mandatory = $true)]
    [string]$WorkDir,
    [string]$AndroidNdkPath,
    [string]$OutputArchive,
    [switch]$RegenerateOperatorConfig
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-Native {
    param([string]$FilePath, [string[]]$Arguments)
    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$FilePath failed with exit code $LASTEXITCODE"
    }
}

function Get-Sha256 {
    param([string]$Path)
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-Identity {
    param([string]$Path, [long]$ByteCount, [string]$Sha256)
    $item = Get-Item -LiteralPath $Path
    if ($item.Length -ne $ByteCount) {
        throw "$Path byte count differs: expected=$ByteCount actual=$($item.Length)"
    }
    $actual = Get-Sha256 $Path
    if ($actual -ne $Sha256) {
        throw "$Path SHA-256 differs: expected=$Sha256 actual=$actual"
    }
}

function Clone-PinnedSource {
    param([string]$Repository, [string]$Commit, [string]$Destination)
    Invoke-Native git @("clone", "--filter=blob:none", "--no-checkout", $Repository, $Destination)
    Invoke-Native git @("-C", $Destination, "checkout", "--detach", $Commit)
    $actual = (& git -C $Destination rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $actual -ne $Commit) {
        throw "Pinned checkout differs for $Repository"
    }
}

function Get-NormalizedOperators {
    param([string]$Path)
    (Get-Content -LiteralPath $Path) |
        ForEach-Object { $_.Trim() } |
        Where-Object { $_ -and -not $_.StartsWith("#") }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$specRoot = Join-Path $repoRoot "mobile\native\sherpa-runtime"
$contractPath = Join-Path $specRoot "build-contract.json"
$contract = Get-Content -Raw -LiteralPath $contractPath | ConvertFrom-Json
$operatorConfig = Join-Path $specRoot $contract.operatorGeneration.configFile
$operatorModels = Join-Path $specRoot $contract.operatorGeneration.modelsFile
$sourcePatch = Join-Path $specRoot $contract.sourcePatch.file

Assert-Identity $operatorConfig (Get-Item $operatorConfig).Length `
    $contract.operatorGeneration.configSha256
Assert-Identity $operatorModels (Get-Item $operatorModels).Length `
    $contract.operatorGeneration.modelsSha256
Assert-Identity $sourcePatch (Get-Item $sourcePatch).Length $contract.sourcePatch.sha256

$resolvedWorkParent = Split-Path -Parent ([IO.Path]::GetFullPath($WorkDir))
if (-not (Test-Path -LiteralPath $resolvedWorkParent)) {
    New-Item -ItemType Directory -Path $resolvedWorkParent | Out-Null
}
$resolvedWork = [IO.Path]::GetFullPath($WorkDir)
if ((Test-Path -LiteralPath $resolvedWork) -and
    (Get-ChildItem -Force -LiteralPath $resolvedWork | Select-Object -First 1)) {
    throw "WorkDir must be absent or empty: $resolvedWork"
}
New-Item -ItemType Directory -Force -Path $resolvedWork | Out-Null

if (-not $AndroidNdkPath) {
    if ($env:ANDROID_NDK_HOME) {
        $AndroidNdkPath = $env:ANDROID_NDK_HOME
    } elseif ($env:LOCALAPPDATA) {
        $AndroidNdkPath = Join-Path $env:LOCALAPPDATA `
            "Android\Sdk\ndk\$($contract.ndkVersion)"
    }
}
if (-not $AndroidNdkPath -or -not (Test-Path -LiteralPath $AndroidNdkPath)) {
    throw "Android NDK $($contract.ndkVersion) was not found; pass -AndroidNdkPath"
}
$ndkProperties = Get-Content -LiteralPath (Join-Path $AndroidNdkPath "source.properties")
if ($ndkProperties -notmatch "Pkg.Revision\s*=\s*$([regex]::Escape($contract.ndkVersion))") {
    throw "Android NDK version differs from build-contract.json"
}
foreach ($command in @("git", "cmake", "ninja", "py")) {
    if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
        throw "Required command is missing: $command"
    }
}

$sherpaSource = Join-Path $resolvedWork "sherpa-onnx"
$ortSource = Join-Path $resolvedWork "onnxruntime"
$bundleSource = Join-Path $resolvedWork "onnxruntime-build"
Clone-PinnedSource $contract.sources.sherpaOnnx.repository `
    $contract.sources.sherpaOnnx.commit $sherpaSource
Clone-PinnedSource $contract.sources.onnxRuntime.repository `
    $contract.sources.onnxRuntime.commit $ortSource
Invoke-Native git @("-C", $ortSource, "submodule", "update", "--init", "--recursive")
Clone-PinnedSource $contract.sources.staticBundleRecipe.repository `
    $contract.sources.staticBundleRecipe.commit $bundleSource

if ($RegenerateOperatorConfig) {
    $modelRoot = Join-Path $resolvedWork "operator-models"
    New-Item -ItemType Directory -Force -Path $modelRoot | Out-Null
    $modelContract = Get-Content -Raw -LiteralPath $operatorModels | ConvertFrom-Json
    foreach ($model in $modelContract.models) {
        $familyRoot = Join-Path $modelRoot $model.family
        New-Item -ItemType Directory -Force -Path $familyRoot | Out-Null
        foreach ($file in $model.files) {
            $target = Join-Path $familyRoot $file.name
            Invoke-WebRequest -Uri "$($model.baseUrl)/$($file.name)" -OutFile $target
            Assert-Identity $target $file.byteCount $file.sha256
        }
    }
    $pythonPackages = Join-Path $resolvedWork "python-packages"
    Invoke-Native py @(
        "-3", "-m", "pip", "install", "--disable-pip-version-check",
        "--target", $pythonPackages, "onnxruntime==1.23.2"
    )
    $previousPythonPath = $env:PYTHONPATH
    try {
        $env:PYTHONPATH = $pythonPackages
        $optimizedRoot = Join-Path $resolvedWork "optimized-models"
        Invoke-Native py @(
            "-3", (Join-Path $ortSource "tools\python\convert_onnx_models_to_ort.py"),
            "--output_dir", $optimizedRoot,
            "--optimization_style", $contract.operatorGeneration.optimizationLevel,
            "--target_platform", "arm", $modelRoot
        )
    } finally {
        $env:PYTHONPATH = $previousPythonPath
    }
    $generated = Join-Path $optimizedRoot "required_operators.config"
    $expectedLines = @(Get-NormalizedOperators $operatorConfig)
    $actualLines = @(Get-NormalizedOperators $generated)
    if (Compare-Object $expectedLines $actualLines) {
        throw "Regenerated ONNX Runtime operator contract differs"
    }
}

Invoke-Native git @("-C", $sherpaSource, "apply", "--check", $sourcePatch)
Invoke-Native git @("-C", $sherpaSource, "apply", $sourcePatch)

$toolchain = Join-Path $AndroidNdkPath "build\cmake\android.toolchain.cmake"
$toolBin = Join-Path $AndroidNdkPath "toolchains\llvm\prebuilt\windows-x86_64\bin"
$readElf = Join-Path $toolBin "llvm-readelf.exe"
$strip = Join-Path $toolBin "llvm-strip.exe"
$strings = Join-Path $toolBin "llvm-strings.exe"
foreach ($tool in @($toolchain, $readElf, $strip, $strings)) {
    if (-not (Test-Path -LiteralPath $tool)) { throw "NDK tool is missing: $tool" }
}

$workMap = $resolvedWork.Replace("\", "/")
$prefixFlags = "-ffile-prefix-map=$workMap=/sgt-build -fdebug-prefix-map=$workMap=/sgt-build"
$ortBuild = Join-Path $resolvedWork "build-onnxruntime"
$ortInstall = Join-Path $resolvedWork "onnxruntime-install"
Invoke-Native py @(
    "-3", (Join-Path $ortSource "tools\ci_build\reduce_op_kernels.py"),
    $operatorConfig, "--cmake_build_dir", $ortBuild,
    "--is_extended_minimal_build_or_higher"
)
Invoke-Native cmake @(
    "-S", (Join-Path $bundleSource "static_lib"), "-B", $ortBuild, "-G", "Ninja",
    "-DCMAKE_TOOLCHAIN_FILE=$toolchain", "-DANDROID_ABI=$($contract.abi)",
    "-DANDROID_PLATFORM=android-$($contract.androidApi)", "-DCMAKE_BUILD_TYPE=MinSizeRel",
    "-DCMAKE_INSTALL_PREFIX=$ortInstall", "-DONNXRUNTIME_SOURCE_DIR=$ortSource",
    "-DCMAKE_C_FLAGS_MINSIZEREL=-Oz -DNDEBUG $prefixFlags",
    "-DCMAKE_CXX_FLAGS_MINSIZEREL=-Oz -DNDEBUG $prefixFlags",
    "-DCMAKE_INTERPROCEDURAL_OPTIMIZATION=ON", "-Donnxruntime_ENABLE_LTO=ON",
    "-Donnxruntime_REDUCED_OPS_BUILD=ON", "-Donnxruntime_DISABLE_CONTRIB_OPS=OFF",
    "-Donnxruntime_DISABLE_ML_OPS=ON", "-Donnxruntime_DISABLE_RTTI=ON",
    "-Donnxruntime_DISABLE_FLOAT8_TYPES=ON", "-Donnxruntime_DISABLE_OPTIONAL_TYPE=ON",
    "-Donnxruntime_DISABLE_SPARSE_TENSORS=ON", "-Donnxruntime_BUILD_SHARED_LIB=OFF",
    "-Donnxruntime_BUILD_UNIT_TESTS=OFF", "-Donnxruntime_BUILD_BENCHMARKS=OFF",
    "-Donnxruntime_BUILD_JAVA=OFF", "-Donnxruntime_ENABLE_TRAINING=OFF",
    "-Donnxruntime_USE_KLEIDIAI=ON", "--compile-no-warning-as-error"
)
Invoke-Native cmake @("--build", $ortBuild, "--parallel")
Invoke-Native cmake @("--install", $ortBuild)

$sherpaBuild = Join-Path $resolvedWork "build-sherpa"
$priorInclude = $env:SHERPA_ONNXRUNTIME_INCLUDE_DIR
$priorLib = $env:SHERPA_ONNXRUNTIME_LIB_DIR
try {
    $env:SHERPA_ONNXRUNTIME_INCLUDE_DIR = Join-Path $ortInstall "include"
    $env:SHERPA_ONNXRUNTIME_LIB_DIR = Join-Path $ortInstall "lib"
    Invoke-Native cmake @(
        "-S", $sherpaSource, "-B", $sherpaBuild, "-G", "Ninja",
        "-DCMAKE_TOOLCHAIN_FILE=$toolchain", "-DANDROID_ABI=$($contract.abi)",
        "-DANDROID_PLATFORM=android-$($contract.androidApi)", "-DCMAKE_BUILD_TYPE=MinSizeRel",
        "-DCMAKE_C_FLAGS_MINSIZEREL=-Oz -DNDEBUG -ffunction-sections -fdata-sections $prefixFlags",
        "-DCMAKE_CXX_FLAGS_MINSIZEREL=-Oz -DNDEBUG -ffunction-sections -fdata-sections -fvisibility=hidden -fvisibility-inlines-hidden $prefixFlags",
        "-DBUILD_SHARED_LIBS=OFF", "-DSHERPA_ONNX_ENABLE_JNI=ON",
        "-DSHERPA_ONNX_ENABLE_BINARY=OFF", "-DSHERPA_ONNX_ENABLE_TESTS=OFF",
        "-DSHERPA_ONNX_ENABLE_CHECK=OFF", "-DSHERPA_ONNX_ENABLE_C_API=OFF",
        "-DSHERPA_ONNX_BUILD_C_API_EXAMPLES=OFF", "-DSHERPA_ONNX_ENABLE_TTS=OFF",
        "-DSHERPA_ONNX_ENABLE_SPEAKER_DIARIZATION=OFF",
        "-DSHERPA_ONNX_ENABLE_WEBSOCKET=OFF", "-DSHERPA_ONNX_ENABLE_PORTAUDIO=OFF",
        "-DSHERPA_ONNX_ENABLE_GPU=OFF",
        "-DSHERPA_ONNX_USE_PRE_INSTALLED_ONNXRUNTIME_IF_AVAILABLE=ON"
    )
    Invoke-Native cmake @("--build", $sherpaBuild, "--target", "sherpa-onnx-jni", "--parallel")
} finally {
    $env:SHERPA_ONNXRUNTIME_INCLUDE_DIR = $priorInclude
    $env:SHERPA_ONNXRUNTIME_LIB_DIR = $priorLib
}

$unstripped = Join-Path $sherpaBuild "lib\libsherpa-onnx-jni.so"
$outputRoot = Join-Path $resolvedWork "output"
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
$library = Join-Path $outputRoot $contract.artifact.fileName
Copy-Item -LiteralPath $unstripped -Destination $library
Invoke-Native $strip @("--strip-all", $library)

$header = & $readElf -h $library
if ($header -notmatch "Class:\s+ELF64" -or $header -notmatch "Machine:\s+AArch64") {
    throw "Reduced runtime is not an ELF64 AArch64 library"
}
$symbols = & $readElf --dyn-syms --wide $library
$actualExports = @(
    $symbols | Select-String "Java_com_k2fsa_sherpa_onnx_[A-Za-z0-9_]+" |
        ForEach-Object { [regex]::Match($_.Line, "Java_com_k2fsa_sherpa_onnx_[A-Za-z0-9_]+").Value } |
        Sort-Object -Unique
)
$expectedExports = @($contract.jniExports | Sort-Object)
if (Compare-Object $expectedExports $actualExports) {
    throw "JNI export surface differs from build-contract.json"
}
$dynamic = & $readElf -d $library
$actualNeeded = @(
    $dynamic | Select-String "\(NEEDED\).*\[([^]]+)\]" |
        ForEach-Object { [regex]::Match($_.Line, "\[([^]]+)\]").Groups[1].Value } |
        Sort-Object -Unique
)
$expectedNeeded = @($contract.elf.needed | Sort-Object)
if (Compare-Object $expectedNeeded $actualNeeded) {
    throw "ELF DT_NEEDED surface differs from build-contract.json"
}
if ((Get-Item $library).Length -gt $contract.elf.maximumByteCount) {
    throw "Reduced runtime exceeds its size ceiling"
}
$sections = & $readElf -S $library
if ($sections -match "\.debug_" -or $sections -match "\.symtab") {
    throw "Reduced runtime still contains debug or static symbol sections"
}
$embeddedPaths = & $strings $library | Select-String ([regex]::Escape($workMap))
if ($embeddedPaths) { throw "Reduced runtime contains its absolute build root" }

if (-not $OutputArchive) {
    $OutputArchive = Join-Path $outputRoot "sherpa-runtime.zip"
}
$OutputArchive = [IO.Path]::GetFullPath($OutputArchive)
$outputParent = Split-Path -Parent $OutputArchive
New-Item -ItemType Directory -Force -Path $outputParent | Out-Null
$stream = [IO.File]::Open($OutputArchive, [IO.FileMode]::Create)
try {
    $zip = [IO.Compression.ZipArchive]::new(
        $stream, [IO.Compression.ZipArchiveMode]::Create, $false
    )
    try {
        $entry = $zip.CreateEntry(
            $contract.artifact.fileName,
            [IO.Compression.CompressionLevel]::Optimal
        )
        $entry.LastWriteTime = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
        $entryStream = $entry.Open()
        try {
            $input = [IO.File]::OpenRead($library)
            try { $input.CopyTo($entryStream) } finally { $input.Dispose() }
        } finally { $entryStream.Dispose() }
    } finally { $zip.Dispose() }
} finally { $stream.Dispose() }

Write-Host "ELF: $library"
Write-Host "ELF bytes: $((Get-Item $library).Length)"
Write-Host "ELF SHA-256: $(Get-Sha256 $library)"
Write-Host "Archive: $OutputArchive"
Write-Host "Archive bytes: $((Get-Item $OutputArchive).Length)"
Write-Host "Archive SHA-256: $(Get-Sha256 $OutputArchive)"
