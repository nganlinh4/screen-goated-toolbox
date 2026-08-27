[CmdletBinding()]
param(
    [switch]$SkipNpmInstall,
    [string]$OutputDir
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

function Build-Frontend {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Project,
        [string]$Target
    )

    $projectRoot = Join-Path $repoRoot $Project
    Push-Location $projectRoot
    try {
        if (-not $SkipNpmInstall -and -not (Test-Path "node_modules")) {
            & npm.cmd install
            if ($LASTEXITCODE -ne 0) {
                throw "$Name npm install failed"
            }
        }
        & npm.cmd run build
        if ($LASTEXITCODE -ne 0) {
            throw "$Name build failed"
        }
    }
    finally {
        Pop-Location
    }

    if ([string]::IsNullOrEmpty($Target)) {
        return
    }
    $sourceRoot = Join-Path $projectRoot "dist"
    $targetRoot = Join-Path $repoRoot $Target
    New-Item -ItemType Directory -Path (Join-Path $targetRoot "assets") -Force | Out-Null
    foreach ($relative in @("index.html", "assets\index.css", "assets\index.js")) {
        $source = Join-Path $sourceRoot $relative
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "$Name build is missing $relative"
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $targetRoot $relative) -Force
    }
}

Push-Location $repoRoot
try {
    Build-Frontend -Name "PromptDJ" -Project "promptdj-midi" -Target "src\overlay\prompt_dj\dist"
    Build-Frontend -Name "TTS Playground" -Project "tts-playground-ui" -Target "src\overlay\tts_playground\dist"
    $packageArguments = @((Join-Path $PSScriptRoot "package_web_assets.py"))
    if (-not [string]::IsNullOrWhiteSpace($OutputDir)) {
        $packageArguments += @("--output-dir", [IO.Path]::GetFullPath($OutputDir))
    }
    & py -3 @packageArguments
    if ($LASTEXITCODE -ne 0) {
        throw "web asset packaging failed"
    }
}
finally {
    Pop-Location
}
