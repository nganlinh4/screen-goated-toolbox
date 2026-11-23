$ErrorActionPreference = "Stop"
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$worker = Join-Path $repo "native\recorder_worker"
$files = @(
    (Join-Path $worker "build.rs")
    Get-ChildItem (Join-Path $worker "src") -Recurse -File -Filter "*.rs" |
        ForEach-Object { $_.FullName }
)

# The worker imports host modules with #[path]; check its own files directly
# so rustfmt does not resolve host-only modules through the worker's paths.
& rustfmt --edition 2024 --config skip_children=true --check @files
if ($LASTEXITCODE -ne 0) {
    throw "Recorder worker formatting check failed"
}
