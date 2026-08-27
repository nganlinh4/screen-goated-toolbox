[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet(
        "creation",
        "web-assets",
        "recorder",
        "computer-control",
        "local-asr",
        "vc-runtime",
        "qwen-runtime",
        "external-tools"
    )]
    [string]$Component,
    [string[]]$Select = @(),
    [switch]$Stage,
    [string]$CacheRoot,
    [ValidateRange(5, 200)]
    [int]$CacheLimitGiB = 28,
    [switch]$SkipNpmInstall
)

$ErrorActionPreference = "Stop"
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$cacheScript = Join-Path $PSScriptRoot "dev-cache.ps1"
$pathArgs = @{ Action = "Path"; Lane = "package" }
if (-not [string]::IsNullOrWhiteSpace($CacheRoot)) {
    $pathArgs.CacheRoot = $CacheRoot
}
$cargoTarget = (& $cacheScript @pathArgs | Select-Object -Last 1)
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($cargoTarget)) {
    throw "Could not resolve the package Cargo cache"
}
$resolvedCache = Split-Path -Parent (Split-Path -Parent $cargoTarget)
& $cacheScript -Action Prune -CacheRoot $resolvedCache -MaxGiB $CacheLimitGiB `
    -ProtectLane package -Apply
if ($LASTEXITCODE -ne 0) {
    throw "Development cache maintenance failed"
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$output = Join-Path $resolvedCache "packages\jobs\$Component\$stamp"
New-Item -ItemType Directory -Path $output -Force | Out-Null
$env:SGT_DEV_CACHE_ROOT = $resolvedCache
Remove-Item Env:SGT_COMPONENT_DELIVERY_CHANNEL -ErrorAction SilentlyContinue
Remove-Item Env:SGT_STAGING_DELIVERY_ROOT -ErrorAction SilentlyContinue

$manifestName = $null
$trackedRelative = $null
switch ($Component) {
    "creation" {
        & (Join-Path $PSScriptRoot "build-creation-windows-pack.ps1") `
            -OutputDir $output -CargoTargetDir $cargoTarget `
            -SkipNpmInstall:$SkipNpmInstall
        $manifestName = "sgt_creation_windows.packages.json"
        $trackedRelative = "component-delivery/windows/creation-v1.json"
    }
    "web-assets" {
        $arguments = @{ OutputDir = $output }
        if ($SkipNpmInstall) { $arguments.SkipNpmInstall = $true }
        & (Join-Path $PSScriptRoot "build-web-asset-packs.ps1") @arguments
        $manifestName = "sgt_web_assets.packages.json"
        $trackedRelative = "component-delivery/windows/web-assets-v1.json"
    }
    "recorder" {
        & (Join-Path $PSScriptRoot "build-recorder-component-packs.ps1") `
            -OutputDir $output -CargoTargetDir $cargoTarget
        $manifestName = "sgt_recorder.packages.json"
        $trackedRelative = "component-delivery/windows/recorder-v1.json"
    }
    "computer-control" {
        & (Join-Path $PSScriptRoot "build-computer-control-engine-pack.ps1") `
            -OutputDir $output -CargoTargetDir $cargoTarget
        $manifestName = "sgt_computer_control.packages.json"
        $trackedRelative = "component-delivery/windows/computer-control-v1.json"
    }
    "local-asr" {
        & (Join-Path $PSScriptRoot "build-local-asr-packs.ps1") `
            -OutputDir $output -CargoTargetDir $cargoTarget
        $manifestName = "sgt_local_asr.packages.json"
        $trackedRelative = "component-delivery/windows/local-asr-v1.json"
    }
    "vc-runtime" {
        & (Join-Path $PSScriptRoot "build-vc-runtime-pack.ps1") -OutputDir $output
        $manifestName = "sgt_vc_runtime.packages.json"
        $trackedRelative = "component-delivery/windows/vc-runtime-v1.json"
    }
    "qwen-runtime" {
        & (Join-Path $PSScriptRoot "build-qwen3-runtime-pack.ps1") -OutputDir $output
        $manifestName = "sgt_qwen3_runtime.packages.json"
        $trackedRelative = "component-delivery/windows/qwen-runtime-v1.json"
    }
    "external-tools" {
        & (Join-Path $PSScriptRoot "build-external-tool-packs.ps1") -OutputDir $output
        $manifestName = "sgt_external_tools.packages.json"
        $trackedRelative = "component-delivery/windows/external-tools-v1.json"
    }
}
if ($LASTEXITCODE -ne 0) {
    throw "$Component candidate packaging failed"
}

$packageManifest = Join-Path $output $manifestName
if (-not (Test-Path -LiteralPath $packageManifest -PathType Leaf)) {
    throw "$Component packager did not produce $packageManifest"
}
Write-Host "Candidate package: $packageManifest" -ForegroundColor Green

if ($Stage) {
    $arguments = @(
        (Join-Path $PSScriptRoot "component_release.py"),
        "--cache-root", $resolvedCache,
        "stage",
        "--repo-root", $repo,
        "--package-manifest", $packageManifest,
        "--asset-root", $output,
        "--tracked-manifest", (Join-Path $repo $trackedRelative),
        "--contract-relative", $trackedRelative
    )
    foreach ($identifier in $Select) {
        $arguments += @("--select", $identifier)
    }
    & py -3 @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Component staging publish failed"
    }
    Write-Host "Run .\run-dev.ps1 -UseStagingDelivery to test the verified candidate." `
        -ForegroundColor Cyan
}
