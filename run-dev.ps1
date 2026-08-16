[CmdletBinding()]
param(
    [switch]$SkipFrontendBuild,
    [switch]$SkipNpmInstall,
    [switch]$SkipCreationRuntimeBuild,
    [switch]$BuildLocalCreationRuntime,
    [switch]$UseStagingDelivery,
    [string]$DevCacheRoot,
    [ValidateRange(5, 200)]
    [int]$DevCacheLimitGiB = 28,
    [switch]$SkipCacheMaintenance,
    [string]$CargoCommand = "run",
    [string[]]$CargoArgs = @(),
    [int]$Tail = 120
)

$ErrorActionPreference = "Stop"

$repoRoot = $PSScriptRoot
$cacheScript = Join-Path $repoRoot "scripts\dev-cache.ps1"
$cacheArguments = @{ Action = "Path"; Lane = "dev" }
if (-not [string]::IsNullOrWhiteSpace($DevCacheRoot)) {
    $cacheArguments.CacheRoot = $DevCacheRoot
}
$devCargoTarget = (& $cacheScript @cacheArguments | Select-Object -Last 1)
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($devCargoTarget)) {
    throw "Could not resolve the managed SGT development cache"
}
$cacheRootResolved = Split-Path -Parent (Split-Path -Parent $devCargoTarget)
if (-not $SkipCacheMaintenance) {
    & $cacheScript -Action Prune -CacheRoot $cacheRootResolved `
        -MaxGiB $DevCacheLimitGiB -ProtectLane dev -Apply
    if ($LASTEXITCODE -ne 0) {
        throw "SGT development cache maintenance failed"
    }
}
$env:SGT_DEV_CACHE_ROOT = $cacheRootResolved
$env:CARGO_TARGET_DIR = $devCargoTarget
Remove-Item Env:SGT_SCREEN_TRANSLATE_AUTO_EVIDENCE -ErrorAction SilentlyContinue
$logDir = Join-Path $cacheRootResolved "evidence\dev-run-logs"

if ($UseStagingDelivery) {
    $stagingRoot = Join-Path $cacheRootResolved "staging\contracts"
    if (-not (Test-Path -LiteralPath $stagingRoot -PathType Container)) {
        throw "No staged delivery contracts exist at $stagingRoot. Stage a component first."
    }
    $env:SGT_COMPONENT_DELIVERY_CHANNEL = "staging"
    $env:SGT_STAGING_DELIVERY_ROOT = $stagingRoot
    $env:SGT_RUNTIME_STATE_ROOT = Join-Path $cacheRootResolved "runtime\staging"
}
else {
    Remove-Item Env:SGT_COMPONENT_DELIVERY_CHANNEL -ErrorAction SilentlyContinue
    Remove-Item Env:SGT_STAGING_DELIVERY_ROOT -ErrorAction SilentlyContinue
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$cargoLog = Join-Path $logDir "cargo-$CargoCommand-$stamp.log"

function Write-Section {
    param([string]$Title)

    Write-Host ""
    Write-Host "===== $Title =====" -ForegroundColor Cyan
}

function Run-Npm {
    param(
        [string]$Dir,
        [string[]]$ArgsList
    )

    Push-Location $Dir
    try {
        & npm.cmd @ArgsList
        if ($LASTEXITCODE -ne 0) {
            throw "npm $($ArgsList -join ' ') failed in $Dir"
        }
    }
    finally {
        Pop-Location
    }
}

function Sync-Frontend {
    param(
        [string]$Name,
        [string]$SourceRelative,
        [string]$TargetRelative,
        [switch]$BuildsDirectlyToTarget
    )

    $source = Join-Path $repoRoot $SourceRelative
    $dist = Join-Path $source "dist"
    $target = Join-Path $repoRoot $TargetRelative

    Write-Section "Building $Name"
    if (-not $SkipNpmInstall) {
        Run-Npm $source @("install")
    }
    $buildStartedUtc = [DateTime]::UtcNow.AddSeconds(-2)
    Run-Npm $source @("run", "build")

    if ($BuildsDirectlyToTarget) {
        $newestTarget = Get-ChildItem -LiteralPath $target -File -Recurse -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTimeUtc -Descending |
            Select-Object -First 1
        if (-not $newestTarget -or $newestTarget.LastWriteTimeUtc -lt $buildStartedUtc) {
            throw "$Name build did not refresh $target"
        }
        Write-Host "$Name assets built directly into $target" -ForegroundColor Green
        return
    }

    $newestDist = Get-ChildItem -LiteralPath $dist -File -Recurse -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if (-not $newestDist -or $newestDist.LastWriteTimeUtc -lt $buildStartedUtc) {
        throw "$Name build did not refresh $dist; mark direct-to-target builds explicitly"
    }

    if (Test-Path $target) {
        Remove-Item $target -Recurse -Force
    }
    New-Item -ItemType Directory -Path $target -Force | Out-Null
    Copy-Item "$dist\*" -Destination $target -Recurse -Force
    Write-Host "$Name assets copied to $target" -ForegroundColor Green
}

function Build-CreationRuntime {
    $runtimeRoot = Join-Path $repoRoot "native\sgt_3d_generator_runtime"
    $runtimeManifest = Join-Path $runtimeRoot "Cargo.toml"
    if (-not (Test-Path -LiteralPath $runtimeManifest)) {
        Write-Host "Private creation runtime checkout not present; using managed delivery." -ForegroundColor Yellow
        return
    }

    Write-Section "Building Creation Runtime"
    $runtimeTarget = Join-Path $cacheRootResolved "cargo\creation-runtime-dev"
    Push-Location $runtimeRoot
    try {
        & cargo.exe build --locked --target-dir $runtimeTarget
        if ($LASTEXITCODE -ne 0) {
            throw "Creation runtime debug build failed. Close any running development app and retry."
        }
    }
    finally {
        Pop-Location
    }

    $runtimeExe = Join-Path $runtimeTarget "debug\sgt_creation_runtime.exe"
    if (-not (Test-Path -LiteralPath $runtimeExe -PathType Leaf)) {
        throw "Creation runtime build did not produce $runtimeExe"
    }

    $sourceFiles = @(
        Get-ChildItem -LiteralPath (Join-Path $runtimeRoot "src") -File -Recurse
        Get-Item -LiteralPath $runtimeManifest
        Get-Item -LiteralPath (Join-Path $runtimeRoot "Cargo.lock")
    )
    $newestSource = $sourceFiles | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
    $runtimeFile = Get-Item -LiteralPath $runtimeExe
    if ($runtimeFile.LastWriteTimeUtc -lt $newestSource.LastWriteTimeUtc) {
        throw "Creation runtime executable is older than $($newestSource.FullName)"
    }

    Write-Host "Creation runtime is current: $runtimeExe" -ForegroundColor Green
}

function Quote-CmdArg {
    param([string]$Value)

    if ($Value -match '^[A-Za-z0-9_./:=+-]+$') {
        return $Value
    }
    return '"' + ($Value -replace '"', '\"') + '"'
}

Push-Location $repoRoot
try {
    if ($BuildLocalCreationRuntime -and $SkipCreationRuntimeBuild) {
        throw "Use only one of -BuildLocalCreationRuntime or -SkipCreationRuntimeBuild"
    }
    if ($BuildLocalCreationRuntime) {
        Build-CreationRuntime
    }

    if (-not $SkipFrontendBuild) {
        Sync-Frontend "PromptDJ" "promptdj-midi" "src\overlay\prompt_dj\dist"
        Sync-Frontend "Translation Gummy" "translation-gummy-ui" "src\overlay\translation_gummy\dist"
        Sync-Frontend "Screen Record" "screen-record" "src\overlay\screen_record\dist"
        Sync-Frontend "3D Generator" "3d-generator-ui" "src\overlay\three_d_generator\dist" -BuildsDirectlyToTarget
        Sync-Frontend "Image to SVG" "image-to-svg-ui" "src\overlay\image_to_svg\dist" -BuildsDirectlyToTarget
        Sync-Frontend "Image Creator" "image-creator-ui" "src\overlay\image_creator\dist" -BuildsDirectlyToTarget
        Sync-Frontend "TTS Playground" "tts-playground-ui" "src\overlay\tts_playground\dist"
    }

    New-Item -ItemType Directory -Path $logDir -Force | Out-Null
    $cargoInvocation = @($CargoCommand) + $CargoArgs

    Write-Section "Running cargo $($cargoInvocation -join ' ')"
    Write-Host "Managed Cargo target: $devCargoTarget" -ForegroundColor DarkGray
    if ($UseStagingDelivery) {
        Write-Host "Component delivery: isolated staging channel" -ForegroundColor Magenta
    }
    Write-Host "Cargo output log: $cargoLog" -ForegroundColor Yellow
    Write-Host ""

    $cargoCmdLine = ((@("cargo.exe") + $cargoInvocation) | ForEach-Object { Quote-CmdArg $_ }) -join " "
    & cmd.exe /d /s /c "$cargoCmdLine 2>&1" | Tee-Object -FilePath $cargoLog
    $cargoExitCode = $LASTEXITCODE
}
finally {
    Pop-Location
}

Write-Host ""
Write-Host "===== Cargo output saved =====" -ForegroundColor Yellow
Write-Host $cargoLog

if (Test-Path $cargoLog) {
    Write-Host ""
    Write-Host "===== Last $Tail cargo log lines =====" -ForegroundColor Yellow
    Get-Content $cargoLog -Tail $Tail
}

if ($cargoExitCode -ne 0) {
    Write-Host ""
    Write-Host "cargo exited with code $cargoExitCode" -ForegroundColor Red
    exit $cargoExitCode
}

exit 0
