<#
.SYNOPSIS
    Builds the Moonshine Voice wrapper DLL from the SDK static libraries.

.DESCRIPTION
    The Moonshine SDK ships static .lib files compiled with /MD (dynamic CRT).
    This script compiles a thin wrapper DLL that re-exports the Moonshine C API,
    bridging the CRT gap with the main application (which uses /MT).

    Prerequisites:
    - Visual Studio Build Tools 2022 (cl.exe in PATH)
    - Moonshine SDK downloaded to third_party/moonshine-voice/

.EXAMPLE
    .\scripts\build_moonshine_wrapper.ps1
    .\scripts\build_moonshine_wrapper.ps1 -CopyToPrivateBin
#>

param(
    [switch]$CopyToPrivateBin
)

$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$WrapperDir = Join-Path $RepoRoot "native\moonshine_wrapper"
$SdkDir = Join-Path $RepoRoot "third_party\moonshine-voice\moonshine-voice-windows-x86_64"
$IncludeDir = Join-Path $SdkDir "include"
$LibDir = Join-Path $SdkDir "lib"
$DistDir = Join-Path $RepoRoot "dist\moonshine-runtime-windows-x64"

# Verify SDK exists
if (-not (Test-Path $LibDir)) {
    Write-Error "Moonshine SDK not found at $SdkDir. Download it first:
    curl -L -o moonshine-win.tar.gz https://github.com/moonshine-ai/moonshine/releases/latest/download/moonshine-voice-windows-x86_64.tar.gz
    tar xzf moonshine-win.tar.gz -C third_party/moonshine-voice"
    exit 1
}

# Find cl.exe via vswhere
$vsInstall = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" `
    -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
    -property installationPath 2>$null

if (-not $vsInstall) {
    Write-Error "Visual Studio Build Tools not found. Install 'Desktop development with C++' workload."
    exit 1
}

# Import VS environment
$vcvarsall = Join-Path $vsInstall "VC\Auxiliary\Build\vcvarsall.bat"
Write-Host "Importing VS environment from: $vcvarsall"

# Run vcvarsall and capture environment
$envBlock = cmd /c "`"$vcvarsall`" x64 && set" 2>&1
foreach ($line in $envBlock) {
    if ($line -match "^([^=]+)=(.*)$") {
        [Environment]::SetEnvironmentVariable($matches[1], $matches[2], "Process")
    }
}

# Build
Write-Host "`nCompiling moonshine_wrapper.dll..."
Push-Location $WrapperDir

$clArgs = @(
    "/MD",           # Dynamic CRT (matches Moonshine SDK)
    "/LD",           # Create DLL
    "/O2",           # Optimize for speed
    "/I`"$IncludeDir`"",
    "moonshine_wrapper.c",
    "/link",
    "/LIBPATH:`"$LibDir`"",
    "moonshine.lib",
    "bin-tokenizer.lib",
    "moonshine-utils.lib",
    "ort-utils.lib",
    "onnxruntime.lib",
    "ole32.lib",
    "mmdevapi.lib",
    "/DEF:moonshine_wrapper.def",
    "/OUT:moonshine_wrapper.dll"
)

$process = Start-Process -FilePath "cl.exe" -ArgumentList $clArgs -NoNewWindow -Wait -PassThru
if ($process.ExitCode -ne 0) {
    Pop-Location
    Write-Error "Compilation failed with exit code $($process.ExitCode)"
    exit 1
}

Pop-Location

# Check output
$dllPath = Join-Path $WrapperDir "moonshine_wrapper.dll"
if (-not (Test-Path $dllPath)) {
    Write-Error "moonshine_wrapper.dll was not created"
    exit 1
}

$dllSize = (Get-Item $dllPath).Length / 1MB
Write-Host "`nmoonshine_wrapper.dll created ($([math]::Round($dllSize, 1)) MB)"

# Create distribution bundle
Write-Host "`nCreating distribution bundle..."
New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
Copy-Item $dllPath $DistDir
Copy-Item (Join-Path $LibDir "onnxruntime.dll") $DistDir

$zipPath = Join-Path $RepoRoot "dist\moonshine-runtime-windows-x64.zip"
if (Test-Path $zipPath) { Remove-Item $zipPath }
Compress-Archive -Path "$DistDir\*" -DestinationPath $zipPath
$zipSize = (Get-Item $zipPath).Length / 1MB
Write-Host "Bundle created: $zipPath ($([math]::Round($zipSize, 1)) MB)"

# Optionally copy to private bin dir
if ($CopyToPrivateBin) {
    $privateBin = Join-Path $env:LOCALAPPDATA "screen-goated-toolbox\bin"
    New-Item -ItemType Directory -Path $privateBin -Force | Out-Null
    Copy-Item $dllPath $privateBin
    Copy-Item (Join-Path $LibDir "onnxruntime.dll") $privateBin
    Write-Host "Copied to $privateBin"
}

Write-Host "`nDone! To test: select 'Moonshine Tiny' from the Windows transcription dropdown."
