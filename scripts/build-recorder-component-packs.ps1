param(
    [switch]$RequireDelivery,
    [string]$OutputDir,
    [string]$CargoTargetDir
)

$ErrorActionPreference = "Stop"
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$frontend = Join-Path $repo "screen-record"
$workerManifest = Join-Path $repo "native\recorder_worker\Cargo.toml"
$managedCacheRoot = if ([string]::IsNullOrWhiteSpace($env:SGT_DEV_CACHE_ROOT)) {
    Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)) "SGT-Development\cache"
} else {
    [IO.Path]::GetFullPath($env:SGT_DEV_CACHE_ROOT)
}
$workerTarget = if ([string]::IsNullOrWhiteSpace($CargoTargetDir)) {
    Join-Path $managedCacheRoot "cargo\package"
} else {
    [IO.Path]::GetFullPath($CargoTargetDir)
}
$workerExe = Join-Path $workerTarget "x86_64-pc-windows-msvc\release\sgt-recorder-worker.exe"
$output = if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    Join-Path $managedCacheRoot "packages\jobs\recorder"
} else {
    [IO.Path]::GetFullPath($OutputDir)
}
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
    "-C",
    "link-arg=/Brepro",
    "--remap-path-prefix=$repo=/sgt",
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
$previousSourceDate = $env:SOURCE_DATE_EPOCH
if (-not [string]::IsNullOrEmpty($previousFlags)) {
    $buildFlags += $previousFlags.Split($separator)
}

Push-Location $frontend
try {
    if (-not (Test-Path "node_modules")) {
        npm install
        if ($LASTEXITCODE -ne 0) {
            throw "Screen Recorder npm install failed"
        }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) {
        throw "Screen Recorder frontend build failed"
    }
}
finally {
    Pop-Location
}

Push-Location $repo
try {
    $env:CARGO_ENCODED_RUSTFLAGS = $buildFlags -join $separator
    $env:SOURCE_DATE_EPOCH = "1704067200"
    cargo build `
        --manifest-path $workerManifest `
        --release `
        --target x86_64-pc-windows-msvc `
        --target-dir $workerTarget `
        --locked
    if ($LASTEXITCODE -ne 0) {
        throw "Screen Recorder worker build failed"
    }

    $arguments = @(
        (Join-Path $repo "scripts\package_recorder_components.py"),
        "--worker-exe", $workerExe,
        "--output-dir", $output
    )
    if ($RequireDelivery) {
        $arguments += "--require-delivery"
    }
    & py -3 @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Screen Recorder package build failed"
    }
}
finally {
    if ($null -eq $previousFlags) {
        Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
    }
    else {
        $env:CARGO_ENCODED_RUSTFLAGS = $previousFlags
    }
    $env:SOURCE_DATE_EPOCH = $previousSourceDate
    Pop-Location
}
