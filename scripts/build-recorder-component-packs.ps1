param(
    [switch]$RequireDelivery
)

$ErrorActionPreference = "Stop"
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$frontend = Join-Path $repo "screen-record"
$workerManifest = Join-Path $repo "native\recorder_worker\Cargo.toml"
$workerExe = Join-Path $repo "native\recorder_worker\target\x86_64-pc-windows-msvc\release\sgt-recorder-worker.exe"
$output = Join-Path $repo "local-runtime-bundles\sgt_recorder"

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
    cargo build `
        --manifest-path $workerManifest `
        --release `
        --target x86_64-pc-windows-msvc `
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
    Pop-Location
}
