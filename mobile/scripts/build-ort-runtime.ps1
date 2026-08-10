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

function Extract-Range {
    param([string]$Source, [string]$Destination, [long]$Offset, [long]$ByteCount)
    $input = [IO.File]::OpenRead($Source)
    try {
        $input.Position = $Offset
        $remaining = $ByteCount
        $output = [IO.File]::Open($Destination, [IO.FileMode]::Create)
        try {
            $buffer = [byte[]]::new(1MB)
            while ($remaining -gt 0) {
                $requested = [Math]::Min($buffer.Length, $remaining)
                $read = $input.Read($buffer, 0, $requested)
                if ($read -le 0) { throw "Unexpected EOF extracting $Destination" }
                $output.Write($buffer, 0, $read)
                $remaining -= $read
            }
        } finally { $output.Dispose() }
    } finally { $input.Dispose() }
}

function Get-ElfExports {
    param([string]$ReadElf, [string]$Library)
    @(& $ReadElf --dyn-syms --wide $Library) |
        Where-Object { $_ -match "\bGLOBAL\b.*\bDEFAULT\b" -and $_ -notmatch "\bUND\b" } |
        ForEach-Object {
            $match = [regex]::Match($_, "\s([A-Za-z_][A-Za-z0-9_]*)(?:@@\S+)?$")
            if ($match.Success) { $match.Groups[1].Value }
        } |
        Sort-Object -Unique
}

function Assert-Elf {
    param(
        [string]$ReadElf,
        [string]$Strings,
        [string]$Library,
        [object]$Contract,
        [string]$ForbiddenPath
    )
    $header = & $ReadElf -h $Library
    if ($header -notmatch "Class:\s+ELF64" -or $header -notmatch "Machine:\s+AArch64") {
        throw "$Library is not ELF64 AArch64"
    }
    $dynamic = @(& $ReadElf -d $Library)
    $needed = @(
        $dynamic | Select-String "\(NEEDED\).*\[([^]]+)\]" |
            ForEach-Object { $_.Matches[0].Groups[1].Value } | Sort-Object -Unique
    )
    if (Compare-Object @($Contract.needed | Sort-Object) $needed) {
        throw "$Library DT_NEEDED surface differs"
    }
    $soname = $dynamic | Select-String "\(SONAME\).*\[([^]]+)\]" | Select-Object -First 1
    if (-not $soname -or $soname.Matches[0].Groups[1].Value -ne $Contract.soname) {
        throw "$Library SONAME differs"
    }
    $exports = @(Get-ElfExports $ReadElf $Library)
    if (Compare-Object @($Contract.exports | Sort-Object) $exports) {
        throw "$Library export surface differs"
    }
    if ((Get-Item -LiteralPath $Library).Length -gt $Contract.maximumByteCount) {
        throw "$Library exceeds its size ceiling"
    }
    $sections = & $ReadElf -S $Library
    if ($sections -match "\.debug" -or $sections -match "\.symtab") {
        throw "$Library retains debug or static symbol sections"
    }
    foreach ($line in (& $ReadElf -l $Library | Select-String "^\s*LOAD")) {
        $alignment = [regex]::Match($line.Line, "(0x[0-9a-fA-F]+)\s*$").Groups[1].Value
        if (-not $alignment -or [Convert]::ToInt64($alignment.Substring(2), 16) -lt 0x4000) {
            throw "$Library has a LOAD segment below 16 KB alignment"
        }
    }
    if (& $Strings $Library | Select-String ([regex]::Escape($ForbiddenPath))) {
        throw "$Library contains its absolute build root"
    }
}

function Write-RuntimeArchive {
    param([string]$Path, [string[]]$Files)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Create)
    try {
        $zip = [IO.Compression.ZipArchive]::new(
            $stream, [IO.Compression.ZipArchiveMode]::Create, $false
        )
        try {
            foreach ($file in $Files) {
                $entry = $zip.CreateEntry(
                    (Split-Path -Leaf $file),
                    [IO.Compression.CompressionLevel]::Optimal
                )
                $entry.LastWriteTime = [DateTimeOffset]::new(
                    1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero
                )
                $entryStream = $entry.Open()
                try {
                    $input = [IO.File]::OpenRead($file)
                    try { $input.CopyTo($entryStream) } finally { $input.Dispose() }
                } finally { $entryStream.Dispose() }
            }
        } finally { $zip.Dispose() }
    } finally { $stream.Dispose() }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$mobileRoot = Join-Path $repoRoot "mobile"
$specRoot = Join-Path $mobileRoot "native\ort-runtime"
$contract = Get-Content -Raw -LiteralPath (Join-Path $specRoot "build-contract.json") |
    ConvertFrom-Json
$operatorConfig = Join-Path $specRoot $contract.operatorGeneration.configFile
$operatorModels = Join-Path $specRoot $contract.operatorGeneration.downloadedModelsFile
$embeddedModels = Join-Path $specRoot $contract.operatorGeneration.embeddedModelsFile
$proxyCmake = Join-Path $mobileRoot $contract.proxy.cmakeFile
$proxySource = Join-Path $mobileRoot $contract.proxy.sourceFile
$smokeSource = Join-Path $mobileRoot $contract.releaseGate.smokeSource
$smokeHeader = Join-Path $mobileRoot $contract.releaseGate.smokeHeader
$smokeInputs = Join-Path $mobileRoot $contract.releaseGate.smokeInputs

Assert-Identity $operatorConfig (Get-Item $operatorConfig).Length `
    $contract.operatorGeneration.configSha256
Assert-Identity $operatorModels (Get-Item $operatorModels).Length `
    $contract.operatorGeneration.downloadedModelsSha256
Assert-Identity $embeddedModels (Get-Item $embeddedModels).Length `
    $contract.operatorGeneration.embeddedModelsSha256
Assert-Identity $proxyCmake (Get-Item $proxyCmake).Length $contract.proxy.cmakeSha256
Assert-Identity $proxySource (Get-Item $proxySource).Length $contract.proxy.sourceSha256
Assert-Identity $smokeSource (Get-Item $smokeSource).Length `
    $contract.releaseGate.smokeSourceSha256
Assert-Identity $smokeHeader (Get-Item $smokeHeader).Length `
    $contract.releaseGate.smokeHeaderSha256
Assert-Identity $smokeInputs (Get-Item $smokeInputs).Length `
    $contract.releaseGate.smokeInputsSha256

$noticeRoot = Join-Path $specRoot "assets\third_party\ort-runtime"
$actualNotices = @(
    Get-ChildItem -LiteralPath $noticeRoot -File |
        ForEach-Object { if ($_.Length -eq 0) { throw "Empty notice: $_" }; $_.Name }
)
if (Compare-Object @($contract.noticeFiles | Sort-Object) @($actualNotices | Sort-Object)) {
    throw "ORT runtime notice files differ from the build contract"
}

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
    $sdk = if ($env:ANDROID_SDK_ROOT) { $env:ANDROID_SDK_ROOT } else { $env:ANDROID_HOME }
    if ($sdk) { $AndroidNdkPath = Join-Path $sdk "ndk\$($contract.ndkVersion)" }
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
$cmakeVersion = (& cmake --version | Select-Object -First 1) -replace "^cmake version ", ""
if ([version]$cmakeVersion -lt [version]"3.28") {
    throw "ONNX Runtime 1.24.2 requires CMake 3.28 or newer"
}

$ortSource = Join-Path $resolvedWork "onnxruntime"
Clone-PinnedSource $contract.source.repository $contract.source.commit $ortSource
Invoke-Native git @("-C", $ortSource, "submodule", "update", "--init", "--recursive")

$embeddedContract = Get-Content -Raw -LiteralPath $embeddedModels | ConvertFrom-Json
$moonshineArchive = Join-Path $mobileRoot "androidApp\libs\$($embeddedContract.sourceArchive.fileName)"
Assert-Identity $moonshineArchive $embeddedContract.sourceArchive.byteCount `
    $embeddedContract.sourceArchive.sha256
$compatRoot = Join-Path $resolvedWork "moonshine-compat"
New-Item -ItemType Directory -Force -Path $compatRoot | Out-Null
$moonshineLibrary = Join-Path $compatRoot $embeddedContract.sourceArchive.entry.fileName
$zip = [IO.Compression.ZipFile]::OpenRead($moonshineArchive)
try {
    $entry = $zip.GetEntry($embeddedContract.sourceArchive.entry.fileName)
    if (-not $entry) { throw "Moonshine compatibility library is missing" }
    $input = $entry.Open()
    try {
        $output = [IO.File]::Open($moonshineLibrary, [IO.FileMode]::Create)
        try { $input.CopyTo($output) } finally { $output.Dispose() }
    } finally { $input.Dispose() }
} finally { $zip.Dispose() }
Assert-Identity $moonshineLibrary $embeddedContract.sourceArchive.entry.byteCount `
    $embeddedContract.sourceArchive.entry.sha256

if ($RegenerateOperatorConfig) {
    $allModelsRoot = Join-Path $resolvedWork "operator-models"
    New-Item -ItemType Directory -Force -Path $allModelsRoot | Out-Null
    $downloadContract = Get-Content -Raw -LiteralPath $operatorModels | ConvertFrom-Json
    foreach ($model in $downloadContract.models) {
        $familyRoot = Join-Path $allModelsRoot $model.family
        New-Item -ItemType Directory -Force -Path $familyRoot | Out-Null
        foreach ($file in $model.files) {
            $target = Join-Path $familyRoot $file.name
            Invoke-WebRequest -Uri "$($model.baseUrl)/$($file.name)" -OutFile $target
            Assert-Identity $target $file.byteCount $file.sha256
        }
    }
    $embeddedRoot = Join-Path $allModelsRoot "embedded"
    New-Item -ItemType Directory -Force -Path $embeddedRoot | Out-Null
    foreach ($model in $embeddedContract.models) {
        $target = Join-Path $embeddedRoot $model.name
        Extract-Range $moonshineLibrary $target $model.fileOffset $model.byteCount
        Assert-Identity $target $model.byteCount $model.sha256
    }
    $pythonPackages = Join-Path $resolvedWork "python-packages"
    Invoke-Native py @(
        "-3", "-m", "pip", "install", "--disable-pip-version-check",
        "--target", $pythonPackages, "onnxruntime==1.24.2"
    )
    $previousPythonPath = $env:PYTHONPATH
    try {
        $env:PYTHONPATH = $pythonPackages
        Invoke-Native py @(
            "-3", (Join-Path $ortSource "tools\python\convert_onnx_models_to_ort.py"),
            "--output_dir", $embeddedRoot, "--optimization_style", "Fixed",
            "--target_platform", "arm", $embeddedRoot
        )
        $generatedConfig = Join-Path $resolvedWork "required-operators.generated.config"
        Invoke-Native py @(
            "-3", (Join-Path $ortSource "tools\python\create_reduced_build_config.py"),
            "-f", "ORT", $allModelsRoot, $generatedConfig
        )
    } finally { $env:PYTHONPATH = $previousPythonPath }
    if (Compare-Object @(Get-NormalizedOperators $operatorConfig) `
        @(Get-NormalizedOperators $generatedConfig)) {
        throw "Regenerated ONNX Runtime operator contract differs"
    }
}

$ortBuild = Join-Path $resolvedWork "build-onnxruntime"
$workMap = $resolvedWork.Replace("\", "/")
$prefixMap = "-ffile-prefix-map=$workMap=/sgt-build"
Invoke-Native py @(
    "-3", (Join-Path $ortSource "tools\ci_build\build.py"),
    "--build_dir", $ortBuild, "--config", "MinSizeRel", "--update", "--build",
    "--parallel", "12", "--skip_tests", "--compile_no_warning_as_error",
    "--build_shared_lib", "--enable_lto", "--android",
    "--android_sdk_path", (Split-Path -Parent (Split-Path -Parent $AndroidNdkPath)),
    "--android_ndk_path", $AndroidNdkPath, "--android_abi", $contract.abi,
    "--android_api", "$($contract.androidApi)", "--cmake_generator", "Ninja",
    "--cmake_path", "cmake", "--include_ops_by_config", $operatorConfig,
    "--disable_ml_ops", "--disable_rtti", "--cmake_extra_defines",
    "onnxruntime_DISABLE_FLOAT8_TYPES=ON", "onnxruntime_DISABLE_OPTIONAL_TYPE=ON",
    "onnxruntime_DISABLE_SPARSE_TENSORS=ON",
    "CMAKE_C_FLAGS_MINSIZEREL=-Oz -DNDEBUG -ffunction-sections -fdata-sections $prefixMap",
    "CMAKE_CXX_FLAGS_MINSIZEREL=-Oz -DNDEBUG -ffunction-sections -fdata-sections $prefixMap"
)

$toolBin = Join-Path $AndroidNdkPath "toolchains\llvm\prebuilt\windows-x86_64\bin"
$readElf = Join-Path $toolBin "llvm-readelf.exe"
$strip = Join-Path $toolBin "llvm-strip.exe"
$strings = Join-Path $toolBin "llvm-strings.exe"
$clang = Join-Path $toolBin "aarch64-linux-android$($contract.androidApi)-clang.cmd"
$toolchain = Join-Path $AndroidNdkPath "build\cmake\android.toolchain.cmake"
foreach ($tool in @($readElf, $strip, $strings, $clang, $toolchain)) {
    if (-not (Test-Path -LiteralPath $tool)) { throw "NDK tool is missing: $tool" }
}

$outputRoot = Join-Path $resolvedWork "output"
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
$realLibrary = Join-Path $outputRoot "libonnxruntime_real.so"
Copy-Item -LiteralPath (Join-Path $ortBuild "MinSizeRel\libonnxruntime.so") `
    -Destination $realLibrary
Invoke-Native $strip @("--strip-all", $realLibrary)

$proxyBuild = Join-Path $resolvedWork "build-proxy"
Invoke-Native cmake @(
    "-S", (Split-Path -Parent $proxyCmake), "-B", $proxyBuild, "-G", "Ninja",
    "-DCMAKE_BUILD_TYPE=MinSizeRel", "-DANDROID_ABI=$($contract.abi)",
    "-DANDROID_PLATFORM=android-$($contract.androidApi)",
    "-DCMAKE_TOOLCHAIN_FILE=$toolchain"
)
Invoke-Native cmake @("--build", $proxyBuild, "--parallel")
$proxyLibrary = Join-Path $outputRoot "libonnxruntime.so"
Copy-Item -LiteralPath (Join-Path $proxyBuild "libonnxruntime.so") -Destination $proxyLibrary
Invoke-Native $strip @("--strip-all", $proxyLibrary)

Assert-Elf $readElf $strings $realLibrary $contract.realElf $workMap
Assert-Elf $readElf $strings $proxyLibrary $contract.proxyElf $workMap

$smokeBinary = Join-Path $outputRoot "moonshine-ort-smoke"
Invoke-Native $clang @(
    "-std=c17", "-Oz", "-flto=thin", "-Wall", "-Wextra", "-Werror",
    "-I", (Split-Path -Parent $smokeHeader),
    $smokeSource, "-L", $compatRoot, "-lmoonshine", "-Wl,--gc-sections",
    "-Wl,-z,max-page-size=16384", "-o", $smokeBinary
)

if (-not $OutputArchive) { $OutputArchive = Join-Path $outputRoot "ort-runtime-candidate.zip" }
$OutputArchive = [IO.Path]::GetFullPath($OutputArchive)
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutputArchive) | Out-Null
$packageFiles = @($contract.packageEntries | ForEach-Object { Join-Path $outputRoot $_ })
Write-RuntimeArchive $OutputArchive $packageFiles
$checkArchive = "$OutputArchive.determinism-check"
try {
    Write-RuntimeArchive $checkArchive $packageFiles
    if (Get-Sha256 $OutputArchive -ne (Get-Sha256 $checkArchive)) {
        throw "Repeated runtime packaging is not deterministic"
    }
} finally {
    if (Test-Path -LiteralPath $checkArchive) { [IO.File]::Delete($checkArchive) }
}

Write-Host "Real ELF: $realLibrary ($((Get-Item $realLibrary).Length) bytes)"
Write-Host "Proxy ELF: $proxyLibrary ($((Get-Item $proxyLibrary).Length) bytes)"
Write-Host "Candidate: $OutputArchive ($((Get-Item $OutputArchive).Length) bytes)"
Write-Host "Candidate SHA-256: $(Get-Sha256 $OutputArchive)"
Write-Host "Physical-device smoke is mandatory before replacing the shipped archive."
