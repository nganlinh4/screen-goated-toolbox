[CmdletBinding()]
param(
    [switch]$SkipFrontendBuild,
    [switch]$SkipNpmInstall,
    [string]$CargoCommand = "run",
    [string[]]$CargoArgs = @(),
    [int]$Tail = 120
)

$ErrorActionPreference = "Stop"

$repoRoot = $PSScriptRoot
$logDir = Join-Path $repoRoot "target\dev-run-logs"
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
        [string]$TargetRelative
    )

    $source = Join-Path $repoRoot $SourceRelative
    $dist = Join-Path $source "dist"
    $target = Join-Path $repoRoot $TargetRelative

    Write-Section "Building $Name"
    if (-not $SkipNpmInstall) {
        Run-Npm $source @("install")
    }
    Run-Npm $source @("run", "build")

    if (-not (Test-Path $dist)) {
        throw "$dist was not created"
    }

    if (Test-Path $target) {
        Remove-Item $target -Recurse -Force
    }
    New-Item -ItemType Directory -Path $target -Force | Out-Null
    Copy-Item "$dist\*" -Destination $target -Recurse -Force
    Write-Host "$Name assets copied to $target" -ForegroundColor Green
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
    if (-not $SkipFrontendBuild) {
        Sync-Frontend "PromptDJ" "promptdj-midi" "src\overlay\prompt_dj\dist"
        Sync-Frontend "Translation Gummy" "translation-gummy-ui" "src\overlay\translation_gummy\dist"
        Sync-Frontend "Screen Record" "screen-record" "src\overlay\screen_record\dist"
        Sync-Frontend "3D Generator" "3d-generator-ui" "src\overlay\three_d_generator\dist"
        Sync-Frontend "Image to SVG" "image-to-svg-ui" "src\overlay\image_to_svg\dist"
        Sync-Frontend "TTS Playground" "tts-playground-ui" "src\overlay\tts_playground\dist"
    }

    New-Item -ItemType Directory -Path $logDir -Force | Out-Null
    $cargoInvocation = @($CargoCommand) + $CargoArgs

    Write-Section "Running cargo $($cargoInvocation -join ' ')"
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
