param(
    [string]$Executable = "target\debug\screen-goated-toolbox.exe",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$resolvedExecutable = if ([System.IO.Path]::IsPathRooted($Executable)) {
    $Executable
} else {
    Join-Path $repoRoot $Executable
}

if (-not $SkipBuild) {
    Push-Location $repoRoot
    try {
        cargo build --bin screen-goated-toolbox
        if ($LASTEXITCODE -ne 0) {
            throw "Desktop build failed with exit code $LASTEXITCODE"
        }
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path -LiteralPath $resolvedExecutable -PathType Leaf)) {
    throw "Compositor test executable not found: $resolvedExecutable"
}

$cases = @(
    @{ Name = "result"; Flag = "--result-compositor-smoke" },
    @{ Name = "status"; Flag = "--status-compositor-smoke" },
    @{ Name = "realtime"; Flag = "--realtime-compositor-smoke" }
)

foreach ($case in $cases) {
    Write-Host "Running $($case.Name) compositor restart smoke..."
    $previousAcceptanceMode = $env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN
    try {
        if ($case.Name -eq "result") {
            $env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN = "1"
        }
        $process = Start-Process `
            -FilePath $resolvedExecutable `
            -ArgumentList $case.Flag `
            -PassThru `
            -Wait `
            -WindowStyle Hidden
    } finally {
        $env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN = $previousAcceptanceMode
    }
    if ($process.ExitCode -ne 0) {
        throw "$($case.Name) compositor smoke failed with exit code $($process.ExitCode)"
    }
}

Write-Host "All isolated compositor restart smokes passed."
