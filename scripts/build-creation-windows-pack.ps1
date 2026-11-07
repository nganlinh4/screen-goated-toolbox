[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$OutputDir,
    [string]$CargoTargetDir,
    [switch]$SkipNpmInstall,
    [switch]$RequireDelivery
)

$ErrorActionPreference = "Stop"
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$runtimeOutput = Join-Path ([IO.Path]::GetFullPath($OutputDir)) "runtime-build"
New-Item -ItemType Directory -Path $runtimeOutput -Force | Out-Null

$runtimeBin = $env:SGT_CREATION_RUNTIME_BIN
if (-not [string]::IsNullOrWhiteSpace($runtimeBin)) {
    $runtimeBin = [IO.Path]::GetFullPath($runtimeBin)
    $runtimeManifest = Join-Path $runtimeBin "sgt_creation_runtime.manifest.json"
    if (-not (Test-Path -LiteralPath $runtimeManifest -PathType Leaf)) {
        throw "SGT_CREATION_RUNTIME_BIN must contain sgt_creation_runtime.manifest.json"
    }
    Copy-Item -Path (Join-Path $runtimeBin "*") -Destination $runtimeOutput -Recurse -Force
}
else {
    if ([string]::IsNullOrWhiteSpace($env:SGT_CREATION_RUNTIME_ROOT)) {
        throw "Set SGT_CREATION_RUNTIME_BIN to a built runtime directory, or SGT_CREATION_RUNTIME_ROOT to a runtime source checkout"
    }
    $runtimeRoot = [IO.Path]::GetFullPath($env:SGT_CREATION_RUNTIME_ROOT)
    if (-not (Test-Path -LiteralPath $runtimeRoot -PathType Container)) {
        throw "SGT_CREATION_RUNTIME_ROOT is not a directory"
    }
    $previousCargoTarget = $env:CARGO_TARGET_DIR
    try {
        if (-not [string]::IsNullOrWhiteSpace($CargoTargetDir)) {
            $env:CARGO_TARGET_DIR = [IO.Path]::GetFullPath($CargoTargetDir)
        }
        # Packaging consumes already-developed source. Development tests run before
        # release work and must never be hidden inside a canonical package build.
        $runtimeArgs = @{ OutDir = $runtimeOutput; SkipTests = $true }
        & (Join-Path $runtimeRoot "scripts\build_exe.ps1") @runtimeArgs
        if ($LASTEXITCODE -ne 0) { throw "Creation runtime build failed" }
    }
    finally {
        if ($null -eq $previousCargoTarget) {
            Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_DIR = $previousCargoTarget
        }
    }
}

Push-Location (Join-Path $repo "3d-generator-ui")
try {
    if (-not $SkipNpmInstall -and -not (Test-Path "node_modules")) {
        & npm.cmd install
        if ($LASTEXITCODE -ne 0) { throw "3D Creation npm install failed" }
    }
    & npm.cmd run build
    if ($LASTEXITCODE -ne 0) { throw "3D Creation frontend build failed" }
}
finally {
    Pop-Location
}

$packageArguments = @(
    (Join-Path $PSScriptRoot "package_creation_windows.py"),
    "--runtime-manifest", (Join-Path $runtimeOutput "sgt_creation_runtime.manifest.json"),
    "--output-dir", ([IO.Path]::GetFullPath($OutputDir))
)
if ($RequireDelivery) { $packageArguments += "--require-delivery" }
& py -3 @packageArguments
if ($LASTEXITCODE -ne 0) { throw "Creation archive packaging failed" }
