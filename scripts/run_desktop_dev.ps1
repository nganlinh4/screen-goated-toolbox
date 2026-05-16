param(
    [switch]$SkipFrontendBuild
)

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

function Invoke-Npm {
    param(
        [string]$Directory,
        [string[]]$Arguments
    )

    Push-Location $Directory
    try {
        & npm.cmd @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "npm $($Arguments -join ' ') failed in $Directory"
        }
    } finally {
        Pop-Location
    }
}

function Build-And-CopyFrontend {
    param(
        [string]$Source,
        [string]$Destination
    )

    Invoke-Npm $Source @("install")
    Invoke-Npm $Source @("run", "build")
    $Dist = Join-Path $Source "dist"
    if (!(Test-Path $Dist)) {
        throw "$Dist was not created"
    }
    if (Test-Path $Destination) {
        Remove-Item $Destination -Recurse -Force
    }
    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    Copy-Item (Join-Path $Dist "*") -Destination $Destination -Recurse -Force
}

Push-Location $RepoRoot
try {
    if (!$SkipFrontendBuild) {
        Build-And-CopyFrontend ".\promptdj-midi" ".\src\overlay\prompt_dj\dist"
        Build-And-CopyFrontend ".\translation-gummy-ui" ".\src\overlay\translation_gummy\dist"
        Build-And-CopyFrontend ".\screen-record" ".\src\overlay\screen_record\dist"
    }

    cargo run
    if ($LASTEXITCODE -ne 0) {
        throw "cargo run failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}
