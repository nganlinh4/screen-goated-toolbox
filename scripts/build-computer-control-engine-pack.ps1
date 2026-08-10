param(
    [switch]$RequireDelivery
)

$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$workerManifest = Join-Path $repoRoot "native\computer_control_engine\Cargo.toml"
$workerExe = Join-Path $repoRoot "native\computer_control_engine\target\x86_64-pc-windows-msvc\release\sgt-computer-control-engine.exe"
$output = Join-Path $repoRoot "local-runtime-bundles\sgt_computer_control"
$separator = [char]0x1f
$cargoCacheRoot = if ($env:CARGO_HOME) {
    [IO.Path]::GetFullPath($env:CARGO_HOME).TrimEnd('\')
}
else {
    Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)) ".cargo"
}
$buildFlags = @(
    "-C",
    "target-feature=+crt-static",
    "--remap-path-prefix=$repoRoot=/sgt",
    "--remap-path-prefix=$cargoCacheRoot=/cargo"
)
$profileRoot = [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)
if (-not [string]::IsNullOrWhiteSpace($profileRoot)) {
    $buildFlags += "--remap-path-prefix=$profileRoot=/build-user"
}
$previousFlags = [Environment]::GetEnvironmentVariable(
    "CARGO_ENCODED_RUSTFLAGS",
    [EnvironmentVariableTarget]::Process
)
if (-not [string]::IsNullOrEmpty($previousFlags)) {
    $buildFlags += $previousFlags.Split($separator)
}

Push-Location $repoRoot
try {
    $env:CARGO_ENCODED_RUSTFLAGS = $buildFlags -join $separator
    cargo build `
        --locked `
        --manifest-path $workerManifest `
        --release `
        --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) {
        throw "Computer Control engine build failed"
    }
    $arguments = @(
        (Join-Path $repoRoot "scripts\package_computer_control_engine.py"),
        "--worker-exe", $workerExe,
        "--output-dir", $output
    )
    if ($RequireDelivery) {
        $arguments += "--require-delivery"
    }
    & py -3 @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Computer Control engine package failed"
    }
}
finally {
    if ($null -eq $previousFlags) {
        Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
    }
    else {
        $env:CARGO_ENCODED_RUSTFLAGS = $previousFlags
    }
    Pop-Location
}
