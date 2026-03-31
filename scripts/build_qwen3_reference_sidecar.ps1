param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [ValidateSet("auto", "cpu", "cu126", "cu128")]
    [string]$Runtime = "auto",
    [string]$AssetName = "qwen3-asr-reference-windows-x64",
    [switch]$Clean
)

$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
    throw "build_qwen3_reference_sidecar.ps1 must run on Windows PowerShell."
}

$vendorDir = Join-Path $RepoRoot "third_party/qwen3-asr-rs"
if (!(Test-Path $vendorDir)) {
    throw "Vendored qwen3-asr-rs source not found at $vendorDir"
}

$distRoot = Join-Path $RepoRoot "dist"
$bundleDir = Join-Path $distRoot $AssetName
$zipPath = Join-Path $distRoot "$AssetName.zip"
$cacheDir = Join-Path $RepoRoot "tools/qwen3-reference-cache"
$variantMarker = Join-Path $cacheDir "runtime-variant.txt"

New-Item -ItemType Directory -Force -Path $distRoot | Out-Null
New-Item -ItemType Directory -Force -Path $cacheDir | Out-Null

if ($Clean) {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $bundleDir
    Remove-Item -Force -ErrorAction SilentlyContinue $zipPath
}

function Resolve-QwenRuntimeVariant {
    param([string]$RequestedRuntime)

    if ($RequestedRuntime -ne "auto") {
        return $RequestedRuntime
    }

    $nvidiaSmi = Get-Command "nvidia-smi.exe" -ErrorAction SilentlyContinue
    if ($null -ne $nvidiaSmi) {
        $gpuNames = @()
        try {
            $gpuNames = & $nvidiaSmi.Source --query-gpu=name --format=csv,noheader 2>$null
        } catch {
            $gpuNames = @()
        }

        foreach ($name in $gpuNames) {
            if ($name -match 'RTX 50' -or $name -match 'Blackwell') {
                return "cu128"
            }
        }

        return "cu126"
    }

    return "cpu"
}

function Get-LibtorchUrl {
    param([string]$Variant)

    switch ($Variant) {
        "cpu" { return "https://download.pytorch.org/libtorch/cpu/libtorch-win-shared-with-deps-2.7.1%2Bcpu.zip" }
        "cu126" { return "https://download.pytorch.org/libtorch/cu126/libtorch-win-shared-with-deps-2.7.1%2Bcu126.zip" }
        "cu128" { return "https://download.pytorch.org/libtorch/cu128/libtorch-win-shared-with-deps-2.7.1%2Bcu128.zip" }
        default { throw "Unsupported libtorch runtime variant: $Variant" }
    }
}

function Test-LibtorchLayout {
    param(
        [string]$Variant,
        [string]$RuntimeDir
    )

    $hasPrimaryTorchHeader = Test-Path (Join-Path $RuntimeDir "include\\torch\\torch.h")
    $hasApiTorchHeader = Test-Path (Join-Path $RuntimeDir "include\\torch\\csrc\\api\\include\\torch\\torch.h")
    if (!($hasPrimaryTorchHeader -or $hasApiTorchHeader)) {
        return $false
    }

    $required = @(
        (Join-Path $RuntimeDir "lib\\torch_cpu.dll"),
        (Join-Path $RuntimeDir "lib\\c10.dll")
    )

    if ($Variant -ne "cpu") {
        $required += @(
            (Join-Path $RuntimeDir "lib\\c10_cuda.dll"),
            (Join-Path $RuntimeDir "lib\\torch_cuda.dll"),
            (Join-Path $RuntimeDir "lib\\torch_cuda.lib")
        )
    }

    foreach ($path in $required) {
        if (!(Test-Path $path)) {
            return $false
        }
    }

    return $true
}

function Resolve-LibtorchRoot {
    param(
        [string]$Variant,
        [string]$VariantDir
    )

    $nestedRoot = Join-Path $VariantDir "libtorch"
    if (Test-LibtorchLayout -Variant $Variant -RuntimeDir $nestedRoot) {
        return $nestedRoot
    }

    return $VariantDir
}

$resolvedRuntime = Resolve-QwenRuntimeVariant -RequestedRuntime $Runtime
$libtorchUrl = Get-LibtorchUrl -Variant $resolvedRuntime
$libtorchZip = Join-Path $cacheDir "libtorch-$resolvedRuntime.zip"
$libtorchDir = Join-Path $cacheDir "libtorch-$resolvedRuntime"

Write-Host "Selected libtorch runtime: $resolvedRuntime"

if (!(Test-Path $libtorchZip)) {
    Write-Host "Downloading libtorch from $libtorchUrl"
    $curl = Get-Command "curl.exe" -ErrorAction SilentlyContinue
    if ($null -ne $curl) {
        & $curl.Source --fail --location --continue-at - --output $libtorchZip $libtorchUrl
    } else {
        Invoke-WebRequest -Uri $libtorchUrl -OutFile $libtorchZip
    }
}

if ((Test-Path $libtorchDir) -and !(Test-LibtorchLayout -Variant $resolvedRuntime -RuntimeDir $libtorchDir)) {
    Write-Host "Cached libtorch layout is incomplete for $resolvedRuntime, removing it"
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $libtorchDir
}

if (!(Test-LibtorchLayout -Variant $resolvedRuntime -RuntimeDir $libtorchDir)) {
    Write-Host "Ensuring libtorch archive is fully downloaded for $resolvedRuntime"
    $curl = Get-Command "curl.exe" -ErrorAction SilentlyContinue
    if ($null -ne $curl) {
        & $curl.Source --fail --location --continue-at - --output $libtorchZip $libtorchUrl
    } elseif (!(Test-Path $libtorchZip)) {
        Invoke-WebRequest -Uri $libtorchUrl -OutFile $libtorchZip
    }

    Write-Host "Extracting libtorch ($resolvedRuntime)"
    $expandedRoot = Join-Path $cacheDir "libtorch"
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $expandedRoot
    $tar = Get-Command "tar.exe" -ErrorAction SilentlyContinue
    if ($null -ne $tar) {
        & $tar.Source -xf $libtorchZip -C $cacheDir
    } else {
        Expand-Archive -Path $libtorchZip -DestinationPath $cacheDir -Force
    }
    if ($expandedRoot -ne $libtorchDir) {
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $libtorchDir
        Move-Item -Path $expandedRoot -Destination $libtorchDir
    }
}

$libtorchRoot = Resolve-LibtorchRoot -Variant $resolvedRuntime -VariantDir $libtorchDir
if (!(Test-LibtorchLayout -Variant $resolvedRuntime -RuntimeDir $libtorchRoot)) {
    throw "Resolved libtorch root '$libtorchRoot' is missing required files for $resolvedRuntime"
}

Set-Content -Path $variantMarker -Value $resolvedRuntime -NoNewline

$env:LIBTORCH = $libtorchRoot
# torch-sys treats the presence of LIBTORCH_USE_PYTORCH as an instruction to
# use the machine Python install, even if the value is "0". Clear it so builds
# always bind to our managed libtorch bundle.
Remove-Item Env:LIBTORCH_USE_PYTORCH -ErrorAction SilentlyContinue
$env:LIBTORCH_BYPASS_VERSION_CHECK = "1"

Write-Host "Building vendored qwen3-asr-rs sidecar"
Push-Location $vendorDir
try {
    cargo build --release --bin asr-server
}
finally {
    Pop-Location
}

$serverExe = Join-Path $vendorDir "target/release/asr-server.exe"
if (!(Test-Path $serverExe)) {
    throw "Expected built sidecar at $serverExe"
}

Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $bundleDir
New-Item -ItemType Directory -Force -Path $bundleDir | Out-Null
Copy-Item $serverExe (Join-Path $bundleDir "asr-server.exe") -Force
$bundleLibtorchRoot = Join-Path $bundleDir "libtorch"
$bundleLibtorchLib = Join-Path $bundleLibtorchRoot "lib"
New-Item -ItemType Directory -Force -Path $bundleLibtorchLib | Out-Null
Copy-Item (Join-Path $libtorchRoot "lib\\*") $bundleLibtorchLib -Recurse -Force

foreach ($metadataName in @("build-version", "build-hash")) {
    $metadataPath = Join-Path $libtorchRoot $metadataName
    if (Test-Path $metadataPath) {
        Copy-Item $metadataPath (Join-Path $bundleLibtorchRoot $metadataName) -Force
    }
}

if (Test-Path $zipPath) {
    try {
        Remove-Item -Force $zipPath
    } catch {
        Write-Warning "Could not remove existing archive '$zipPath': $($_.Exception.Message)"
    }
}

$tempZipPath = Join-Path $distRoot "$AssetName.tmp.zip"
Remove-Item -Force -ErrorAction SilentlyContinue $tempZipPath

Write-Host "Packaging $zipPath"
Compress-Archive -Path (Join-Path $bundleDir "*") -DestinationPath $tempZipPath -Force

try {
    Move-Item -Force $tempZipPath $zipPath
    Write-Host "Qwen3 reference sidecar ready at $zipPath"
} catch {
    Write-Warning "Sidecar bundle is updated at '$bundleDir', but the archive could not be refreshed: $($_.Exception.Message)"
    Remove-Item -Force -ErrorAction SilentlyContinue $tempZipPath
}
