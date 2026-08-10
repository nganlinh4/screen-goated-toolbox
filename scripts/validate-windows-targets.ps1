param(
    [switch]$SkipSetup
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$vswhere = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"

if (!(Test-Path $vswhere)) {
    throw "vswhere.exe not found at $vswhere"
}

$vsPath = & $vswhere -latest -products * -property installationPath
if (!$vsPath) {
    throw "Visual Studio installation not found."
}

function Ensure-RustTarget([string]$Target) {
    $installed = rustup target list --installed
    if ($installed -notcontains $Target) {
        Write-Host "Installing Rust target $Target..."
        rustup target add $Target
    }
}

function Invoke-CargoCheck() {
    $Target = "x86_64-pc-windows-msvc"
    $cmdPath = Join-Path $env:TEMP "sgt-validate-$($Target.Replace('-', '_')).cmd"
    $logPath = Join-Path $repoRoot "target\validation-$($Target.Replace('-', '_')).log"

    $lines = @(
        "@echo off",
        "call `"$vsPath\Common7\Tools\VsDevCmd.bat`" -arch=amd64 -host_arch=x64",
        "cd /d `"$repoRoot`"",
        "cargo check --target $Target > `"$logPath`" 2>&1",
        "exit /b %ERRORLEVEL%"
    )

    Set-Content -Path $cmdPath -Value ($lines -join "`r`n") -Encoding ASCII
    try {
        cmd.exe /c $cmdPath
        if ($LASTEXITCODE -ne 0) {
            throw "cargo check failed for $Target. See $logPath"
        }
        Write-Host "Validated $Target"
    } finally {
        Remove-Item $cmdPath -ErrorAction SilentlyContinue
    }
}

if (-not $SkipSetup) {
    Ensure-RustTarget "x86_64-pc-windows-msvc"
}

Invoke-CargoCheck

Write-Host "Windows x64 target check passed."
