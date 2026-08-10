param(
    [switch]$Debug
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $repo 'native\local_asr_worker\Cargo.toml'
$target = 'x86_64-pc-windows-msvc'
$workerTarget = Join-Path $repo 'native\local_asr_worker\target'
$configuration = if ($Debug) { 'debug' } else { 'release' }
$cargoArgs = @('build', '--manifest-path', $manifest, '--target', $target, '--locked')
if (-not $Debug) {
    $cargoArgs += '--release'
}

$savedRustFlags = $env:RUSTFLAGS
$savedSourceDate = $env:SOURCE_DATE_EPOCH
try {
    $env:RUSTFLAGS = "-C target-feature=+crt-static -C link-arg=/Brepro --remap-path-prefix=$repo=."
    $env:SOURCE_DATE_EPOCH = '1704067200'
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Local ASR worker build failed with exit code $LASTEXITCODE."
    }
} finally {
    $env:RUSTFLAGS = $savedRustFlags
    $env:SOURCE_DATE_EPOCH = $savedSourceDate
}

$worker = Join-Path $workerTarget "$target\$configuration\sgt-local-asr-worker.exe"
if (-not (Test-Path -LiteralPath $worker -PathType Leaf)) {
    throw "Local ASR worker output is missing: $worker"
}
if ($Debug) {
    Write-Output "Development worker ready: $worker"
    exit 0
}

$output = Join-Path $repo 'local-runtime-bundles\sgt_local_asr'
$sources = Join-Path $output 'sources'
New-Item -ItemType Directory -Force -Path $sources | Out-Null
$onnx = Join-Path $sources 'microsoft.ml.onnxruntime.directml.1.24.2.nupkg'
$directml = Join-Path $sources 'microsoft.ai.directml.1.15.4.nupkg'
$downloads = @(
    @{
        Path = $onnx
        Url = 'https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime.directml/1.24.2/microsoft.ml.onnxruntime.directml.1.24.2.nupkg'
        Size = 12411398
        Sha256 = 'c9b8adb96dfb5578097bea42a7d9b7ff8f300fb3c3a6f3052fe5b702628ab681'
    },
    @{
        Path = $directml
        Url = 'https://api.nuget.org/v3-flatcontainer/microsoft.ai.directml/1.15.4/microsoft.ai.directml.1.15.4.nupkg'
        Size = 202292617
        Sha256 = '4e7cb7ddce8cf837a7a75dc029209b520ca0101470fcdf275c1f49736a3615b9'
    }
)
foreach ($download in $downloads) {
    $valid = (Test-Path -LiteralPath $download.Path -PathType Leaf)
    if ($valid) {
        $file = Get-Item -LiteralPath $download.Path
        $valid = $file.Length -eq $download.Size -and
            (Get-FileHash -Algorithm SHA256 -LiteralPath $download.Path).Hash.ToLowerInvariant() -eq $download.Sha256
    }
    if (-not $valid) {
        if (Test-Path -LiteralPath $download.Path) {
            Remove-Item -LiteralPath $download.Path -Force
        }
        Invoke-WebRequest -Uri $download.Url -OutFile $download.Path
    }
    $file = Get-Item -LiteralPath $download.Path
    $digest = (Get-FileHash -Algorithm SHA256 -LiteralPath $download.Path).Hash.ToLowerInvariant()
    if ($file.Length -ne $download.Size -or $digest -ne $download.Sha256) {
        throw "Pinned source package failed integrity verification: $($download.Path)"
    }
}

& py -3 (Join-Path $repo 'scripts\package_local_asr.py') `
    --worker-exe $worker `
    --parakeet-license (Join-Path $repo 'third_party\parakeet-rs\LICENSE') `
    --onnx-package $onnx `
    --directml-package $directml `
    --output-dir $output
if ($LASTEXITCODE -ne 0) {
    throw "Local ASR packaging failed with exit code $LASTEXITCODE."
}
