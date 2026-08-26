param(
    [ValidateSet("Compare", "TrainPgo")]
    [string]$Action = "Compare",
    [switch]$SkipBuild,
    [int]$Runs = 3,
    [int]$TimeoutSeconds = 75,
    [int]$BuildJobs = 2
)

$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$cacheRoot = if ([string]::IsNullOrWhiteSpace($env:SGT_DEV_CACHE_ROOT)) {
    Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)) `
        "SGT-Development\cache"
}
else {
    [IO.Path]::GetFullPath($env:SGT_DEV_CACHE_ROOT)
}
$performanceRoot = Join-Path $cacheRoot "performance"
$targetTriple = "x86_64-pc-windows-msvc"
New-Item -ItemType Directory -Path $performanceRoot -Force | Out-Null
$transientTargets = [Collections.Generic.List[string]]::new()
. (Join-Path $PSScriptRoot "source-fingerprint.ps1")

if ($Runs -lt 1 -or $Runs -gt 20) {
    throw "Runs must be between 1 and 20."
}
if ($BuildJobs -lt 1 -or $BuildJobs -gt 16) {
    throw "BuildJobs must be between 1 and 16."
}

function Invoke-CargoBuild {
    param(
        [Parameter(Mandatory = $true)][string]$Profile,
        [string]$TargetDirectory,
        [string]$EncodedRustFlags
    )

    $savedTarget = $env:CARGO_TARGET_DIR
    $savedFlags = $env:CARGO_ENCODED_RUSTFLAGS
    $savedNonshipping = $env:SGT_NONSHIPPING_PERFORMANCE_BUILD
    try {
        $env:SGT_NONSHIPPING_PERFORMANCE_BUILD = "1"
        if ($TargetDirectory) { $env:CARGO_TARGET_DIR = $TargetDirectory }
        else { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue }
        if ($EncodedRustFlags) { $env:CARGO_ENCODED_RUSTFLAGS = $EncodedRustFlags }
        else { Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue }
        cargo build --locked --jobs $BuildJobs --profile $Profile --target $targetTriple `
            --bin screen-goated-toolbox
        if ($LASTEXITCODE -ne 0) {
            throw "Cargo profile '$Profile' failed with exit code $LASTEXITCODE."
        }
    }
    finally {
        if ($null -eq $savedTarget) { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue }
        else { $env:CARGO_TARGET_DIR = $savedTarget }
        if ($null -eq $savedFlags) { Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue }
        else { $env:CARGO_ENCODED_RUSTFLAGS = $savedFlags }
        if ($null -eq $savedNonshipping) {
            Remove-Item Env:SGT_NONSHIPPING_PERFORMANCE_BUILD -ErrorAction SilentlyContinue
        }
        else { $env:SGT_NONSHIPPING_PERFORMANCE_BUILD = $savedNonshipping }
    }
}

function Remove-TransientTarget {
    param([Parameter(Mandatory = $true)][string]$Path)

    $resolved = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    $allowed = [IO.Path]::GetFullPath($performanceRoot).TrimEnd('\') + '\'
    if (-not $resolved.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to clean a performance path outside $allowed"
    }
    if (Test-Path -LiteralPath $resolved) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}

function Stop-TestProcessTree {
    param([Parameter(Mandatory = $true)][int]$RootId)

    $all = @(Get-CimInstance Win32_Process)
    $ids = [Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add($RootId)
    do {
        $added = $false
        foreach ($process in $all) {
            if ($ids.Contains([int]$process.ParentProcessId) -and
                $ids.Add([int]$process.ProcessId)) {
                $added = $true
            }
        }
    } while ($added)
    $live = @(Get-Process -Id @($ids) -ErrorAction SilentlyContinue)
    if ($live.Count -gt 0) {
        $live | Stop-Process -Force
    }
}

function Save-SmokeFailure {
    param(
        [Parameter(Mandatory = $true)][string]$StateRoot,
        [Parameter(Mandatory = $true)][string]$Build,
        [Parameter(Mandatory = $true)][string]$Smoke,
        [Parameter(Mandatory = $true)][int]$Run,
        [Parameter(Mandatory = $true)][string]$Reason,
        [Parameter(Mandatory = $true)][string]$Executable
    )

    $failureId = [guid]::NewGuid().ToString("N").Substring(0, 8)
    $failureRoot = Join-Path $performanceRoot "failures\$failureId"
    New-Item -ItemType Directory -Path $failureRoot -Force | Out-Null
    $sessionLog = Join-Path $StateRoot "local-app-data\SGT\logs\session.log"
    if (Test-Path -LiteralPath $sessionLog -PathType Leaf) {
        Copy-Item -LiteralPath $sessionLog -Destination (Join-Path $failureRoot "session.log")
    }
    $retainedExecutable = Join-Path $failureRoot "screen-goated-toolbox.exe"
    Copy-Item -LiteralPath $Executable -Destination $retainedExecutable
    [ordered]@{
        generatedAt = (Get-Date).ToUniversalTime().ToString("o")
        build = $Build
        smoke = $Smoke
        run = $Run
        reason = $Reason
        executableSha256 = (Get-FileHash -LiteralPath $retainedExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $failureRoot "failure.json") `
        -Encoding utf8
    Write-Host "Smoke failure evidence: $failureRoot"
}

function Measure-Smoke {
    param(
        [Parameter(Mandatory = $true)][string]$Build,
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$Flag,
        [Parameter(Mandatory = $true)][int]$Run
    )

    $stateId = [guid]::NewGuid().ToString("N").Substring(0, 8)
    $stateRoot = Join-Path $performanceRoot "r\$stateId"
    New-Item -ItemType Directory -Path $stateRoot -Force | Out-Null
    $script:transientTargets.Add($stateRoot)
    $savedStateRoot = $env:SGT_RUNTIME_STATE_ROOT
    $savedOffscreen = $env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN
    try {
        $env:SGT_RUNTIME_STATE_ROOT = $stateRoot
        if ($Flag -eq "--result-compositor-smoke") {
            $env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN = "1"
        }
        else {
            Remove-Item Env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN -ErrorAction SilentlyContinue
        }
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $process = Start-Process -FilePath $Executable -ArgumentList $Flag -PassThru -WindowStyle Hidden
        $peakWorkingSet = 0L
        $peakThreads = 0
        $peakProcesses = 0
        while (-not $process.HasExited -and $timer.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
            Start-Sleep -Milliseconds 250
            $rows = @(Get-CimInstance Win32_Process)
            $ids = [Collections.Generic.HashSet[int]]::new()
            [void]$ids.Add($process.Id)
            do {
                $added = $false
                foreach ($row in $rows) {
                    if ($ids.Contains([int]$row.ParentProcessId) -and
                        $ids.Add([int]$row.ProcessId)) {
                        $added = $true
                    }
                }
            } while ($added)
            $live = @(Get-Process -Id @($ids) -ErrorAction SilentlyContinue)
            $workingSet = ($live | Measure-Object WorkingSet64 -Sum).Sum
            $threads = ($live | ForEach-Object { $_.Threads.Count } | Measure-Object -Sum).Sum
            $peakWorkingSet = [Math]::Max($peakWorkingSet, [long]$workingSet)
            $peakThreads = [Math]::Max($peakThreads, [int]$threads)
            $peakProcesses = [Math]::Max($peakProcesses, $live.Count)
        }
        if (-not $process.HasExited) {
            Stop-TestProcessTree -RootId $process.Id
            $reason = "timed out after $TimeoutSeconds seconds"
            Save-SmokeFailure -StateRoot $stateRoot -Build $Build `
                -Smoke $Flag.TrimStart("-") -Run $Run -Reason $reason `
                -Executable $Executable
            throw "$Build $Flag $reason."
        }
        $timer.Stop()
        $process.Refresh()
        if ($process.ExitCode -ne 0) {
            Save-SmokeFailure -StateRoot $stateRoot -Build $Build `
                -Smoke $Flag.TrimStart("-") -Run $Run `
                -Reason "exit code $($process.ExitCode)" -Executable $Executable
            throw "$Build $Flag failed with exit code $($process.ExitCode)."
        }
        Start-Sleep -Milliseconds 200
        $sessionLog = Join-Path $stateRoot "local-app-data\SGT\logs\session.log"
        $firstPaintValues = @()
        $finalFitValues = @()
        if (Test-Path -LiteralPath $sessionLog -PathType Leaf) {
            foreach ($line in Get-Content -LiteralPath $sessionLog) {
                if ($line -match '\[OverlayPerf\].*phase=first_painted elapsed_ms=([0-9.]+)') {
                    $firstPaintValues += [double]$matches[1]
                }
                if ($line -match '\[OverlayPerf\].*phase=final_fit_completed elapsed_ms=([0-9.]+)') {
                    $finalFitValues += [double]$matches[1]
                }
            }
        }
        $firstPaintValues = @($firstPaintValues | Sort-Object)
        $finalFitValues = @($finalFitValues | Sort-Object)
        [pscustomobject]@{
            build = $Build
            smoke = $Flag.TrimStart("-")
            run = $Run
            elapsedMs = [Math]::Round($timer.Elapsed.TotalMilliseconds, 1)
            peakWorkingSetBytes = $peakWorkingSet
            peakThreads = $peakThreads
            peakProcessCount = $peakProcesses
            firstPaintMedianMs = if ($firstPaintValues.Count) {
                $firstPaintValues[[Math]::Floor($firstPaintValues.Count / 2)]
            } else { $null }
            finalFitMedianMs = if ($finalFitValues.Count) {
                $finalFitValues[[Math]::Floor($finalFitValues.Count / 2)]
            } else { $null }
        }
    }
    finally {
        if ($null -eq $savedStateRoot) {
            Remove-Item Env:SGT_RUNTIME_STATE_ROOT -ErrorAction SilentlyContinue
        }
        else {
            $env:SGT_RUNTIME_STATE_ROOT = $savedStateRoot
        }
        if ($null -eq $savedOffscreen) {
            Remove-Item Env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN `
                -ErrorAction SilentlyContinue
        }
        else {
            $env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN = $savedOffscreen
        }
    }
}

function Invoke-SmokeCorpus {
    param(
        [Parameter(Mandatory = $true)][string]$Build,
        [Parameter(Mandatory = $true)][string]$Executable
    )

    $rows = @()
    foreach ($flag in @(
        "--result-compositor-smoke",
        "--status-compositor-smoke",
        "--realtime-compositor-smoke"
    )) {
        for ($run = 1; $run -le $Runs; $run++) {
            $rows += Measure-Smoke -Build $Build -Executable $Executable -Flag $flag -Run $run
        }
    }
    $rows
}

function Get-ArtifactRecord {
    param(
        [Parameter(Mandatory = $true)][string]$Build,
        [Parameter(Mandatory = $true)][string]$Executable
    )

    $file = Get-Item -LiteralPath $Executable
    [pscustomobject]@{
        build = $Build
        path = $file.FullName
        bytes = $file.Length
        sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

function Remove-OrphanedPerformanceTargets {
    $targetRoot = Join-Path $performanceRoot "t"
    if (-not (Test-Path -LiteralPath $targetRoot -PathType Container)) { return }
    try {
        $processes = @(Get-CimInstance Win32_Process -ErrorAction Stop)
    }
    catch {
        Write-Warning "Could not inspect process command lines; orphan cleanup is disabled: $_"
        return
    }
    $cutoff = [DateTime]::UtcNow.AddMinutes(-15)
    foreach ($directory in @(Get-ChildItem -LiteralPath $targetRoot -Directory -Force)) {
        if ($directory.LastWriteTimeUtc -ge $cutoff) { continue }
        $needle = $directory.FullName.ToLowerInvariant()
        $inUse = $processes | Where-Object {
            ([string]$_.CommandLine).ToLowerInvariant().Contains($needle) -or
            ([string]$_.ExecutablePath).ToLowerInvariant().StartsWith($needle)
        } | Select-Object -First 1
        if (-not $inUse) {
            Remove-TransientTarget -Path $directory.FullName
        }
    }
}

function Assert-SourceFingerprint {
    param(
        [Parameter(Mandatory = $true)][string]$Expected,
        [Parameter(Mandatory = $true)][string]$Phase
    )

    $actual = Get-SgtSourceFingerprint -RepoRoot $repoRoot
    if ($actual -ne $Expected) {
        throw "Source changed during $Phase; expected $Expected, found $actual. Discarding mixed-source performance evidence."
    }
}

Push-Location $repoRoot
try {
    Remove-OrphanedPerformanceTargets
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $sourceFingerprint = Get-SgtSourceFingerprint -RepoRoot $repoRoot
    if ($Action -eq "TrainPgo") {
        $sysroot = (rustc --print sysroot).Trim()
        $llvmProfdata = Join-Path $sysroot "lib\rustlib\x86_64-pc-windows-msvc\bin\llvm-profdata.exe"
        if (-not (Test-Path -LiteralPath $llvmProfdata -PathType Leaf)) {
            throw "llvm-profdata is missing. Run: rustup component add llvm-tools-preview"
        }
        $lane = Join-Path $performanceRoot "pgo-$stamp"
        New-Item -ItemType Directory -Path $lane -Force | Out-Null
        $workId = [guid]::NewGuid().ToString("N").Substring(0, 8)
        $workLane = Join-Path $performanceRoot "t\p$workId"
        $rawProfiles = Join-Path $workLane "r"
        $instrumentedTarget = Join-Path $workLane "i"
        $optimizedTarget = Join-Path $workLane "o"
        New-Item -ItemType Directory -Path $rawProfiles -Force | Out-Null
        $transientTargets.Add($workLane)
        $separator = [char]0x1f
        $cargoHome = if ($env:CARGO_HOME) {
            [IO.Path]::GetFullPath($env:CARGO_HOME).TrimEnd('\')
        }
        else {
            [IO.Path]::GetFullPath((Join-Path $HOME ".cargo")).TrimEnd('\')
        }
        $userProfile = [Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)
        $commonFlags = @(
            "-Ctarget-feature=+crt-static",
            "-Clink-arg=/Brepro",
            "--remap-path-prefix=$repoRoot=/sgt",
            "--remap-path-prefix=$cargoHome=/cargo"
        )
        if (-not [string]::IsNullOrWhiteSpace($userProfile)) {
            $commonFlags += "--remap-path-prefix=$userProfile=/build-user"
        }
        $generateFlags = @(
            $commonFlags
            "-Cprofile-generate=$rawProfiles"
        ) | ForEach-Object { $_ }
        $generateFlags = $generateFlags -join $separator
        Invoke-CargoBuild -Profile release-balanced -TargetDirectory $instrumentedTarget `
            -EncodedRustFlags $generateFlags
        Assert-SourceFingerprint -Expected $sourceFingerprint -Phase "the instrumented PGO build"
        $instrumentedExe = Join-Path $instrumentedTarget `
            "$targetTriple\release-balanced\screen-goated-toolbox.exe"
        $savedProfileFile = $env:LLVM_PROFILE_FILE
        try {
            $env:LLVM_PROFILE_FILE = Join-Path $rawProfiles "default_%m_%p.profraw"
            $training = Invoke-SmokeCorpus -Build "pgo-training" -Executable $instrumentedExe
        }
        finally {
            if ($null -eq $savedProfileFile) {
                Remove-Item Env:LLVM_PROFILE_FILE -ErrorAction SilentlyContinue
            }
            else {
                $env:LLVM_PROFILE_FILE = $savedProfileFile
            }
        }
        Assert-SourceFingerprint -Expected $sourceFingerprint -Phase "PGO training"
        $mergedProfile = Join-Path $workLane "m.profdata"
        $rawFiles = @(Get-ChildItem -LiteralPath $rawProfiles -Filter "*.profraw" -File)
        if ($rawFiles.Count -eq 0) { throw "The training corpus produced no PGO profiles." }
        & $llvmProfdata merge -o $mergedProfile @($rawFiles.FullName)
        if ($LASTEXITCODE -ne 0) { throw "llvm-profdata merge failed." }
        $useFlags = @(
            $commonFlags
            "-Cprofile-use=$mergedProfile",
            "-Cllvm-args=-pgo-warn-missing-function"
        ) | ForEach-Object { $_ }
        $useFlags = $useFlags -join $separator
        Invoke-CargoBuild -Profile release-balanced -TargetDirectory $optimizedTarget `
            -EncodedRustFlags $useFlags
        Assert-SourceFingerprint -Expected $sourceFingerprint -Phase "the optimized PGO build"
        $pgoExe = Join-Path $optimizedTarget `
            "$targetTriple\release-balanced\screen-goated-toolbox.exe"
        $validation = Invoke-SmokeCorpus -Build "pgo" -Executable $pgoExe
        Assert-SourceFingerprint -Expected $sourceFingerprint -Phase "PGO validation"
        $candidateExe = Join-Path $lane "screen-goated-toolbox-pgo.exe"
        $retainedProfile = Join-Path $lane "merged.profdata"
        Copy-Item -LiteralPath $pgoExe -Destination $candidateExe
        Copy-Item -LiteralPath $mergedProfile -Destination $retainedProfile
        $artifact = Get-ArtifactRecord -Build "pgo" -Executable $candidateExe
        $profileArtifact = Get-SgtFileIdentity -Path $retainedProfile
        $contract = Get-Content -LiteralPath (Join-Path $PSScriptRoot "performance-contract.json") `
            -Raw | ConvertFrom-Json
        $maxPgoBytes = [long]($contract.windows.maxShippingBinaryBytes *
            $contract.windows.maxPgoSizeRatio)
        if ($artifact.bytes -gt $maxPgoBytes) {
            throw "PGO binary is $($artifact.bytes) bytes; contract allows $maxPgoBytes."
        }
        $payload = [ordered]@{
            schemaVersion = 2
            generatedAt = (Get-Date).ToUniversalTime().ToString("o")
            rustc = (rustc -vV) -join "`n"
            sourceFingerprint = $sourceFingerprint
            artifact = $artifact
            training = $training
            validation = $validation
            profile = $retainedProfile
            profileArtifact = $profileArtifact
        }
        $report = Join-Path $lane "pgo-report.json"
        $payload | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $report -Encoding utf8
        Write-Host "PGO artifact: $candidateExe"
        Write-Host "PGO report: $report"
        return
    }

    $compareTarget = $null
    if (-not $SkipBuild) {
        $workId = [guid]::NewGuid().ToString("N").Substring(0, 8)
        $compareLane = Join-Path $performanceRoot "t\c$workId"
        $compareTarget = Join-Path $compareLane "b"
        $transientTargets.Add($compareLane)
        $transientTargets.Add($compareTarget)
        Invoke-CargoBuild -Profile release-compact -TargetDirectory $compareTarget
        Assert-SourceFingerprint -Expected $sourceFingerprint -Phase "the compact build"
        Invoke-CargoBuild -Profile release-balanced -TargetDirectory $compareTarget
        Assert-SourceFingerprint -Expected $sourceFingerprint -Phase "the balanced build"
        Invoke-CargoBuild -Profile release-perf -TargetDirectory $compareTarget
        Assert-SourceFingerprint -Expected $sourceFingerprint -Phase "the performance build"
    }
    $artifactRoot = if ($compareTarget) { $compareTarget } else { Join-Path $repoRoot "target" }
    $compactExe = Join-Path $artifactRoot `
        "$targetTriple\release-compact\screen-goated-toolbox.exe"
    $balancedExe = Join-Path $artifactRoot `
        "$targetTriple\release-balanced\screen-goated-toolbox.exe"
    $perfExe = Join-Path $artifactRoot "$targetTriple\release-perf\screen-goated-toolbox.exe"
    foreach ($path in @($compactExe, $balancedExe, $perfExe)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Missing comparison artifact: $path"
        }
    }
    $artifacts = @(
        Get-ArtifactRecord -Build "compact" -Executable $compactExe
        Get-ArtifactRecord -Build "balanced" -Executable $balancedExe
        Get-ArtifactRecord -Build "perf" -Executable $perfExe
    )
    $measurements = @()
    $measurements += Invoke-SmokeCorpus -Build "compact" -Executable $compactExe
    $measurements += Invoke-SmokeCorpus -Build "balanced" -Executable $balancedExe
    $measurements += Invoke-SmokeCorpus -Build "perf" -Executable $perfExe
    Assert-SourceFingerprint -Expected $sourceFingerprint -Phase "the comparison corpus"
    $payload = [ordered]@{
        schemaVersion = 2
        generatedAt = (Get-Date).ToUniversalTime().ToString("o")
        rustc = (rustc -vV) -join "`n"
        sourceFingerprint = $sourceFingerprint
        artifacts = $artifacts
        measurements = $measurements
    }
    $report = Join-Path $performanceRoot "windows-$stamp.json"
    $payload | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $report -Encoding utf8
    & (Join-Path $PSScriptRoot "validate-windows-performance-report.ps1") `
        -Report $report -ExpectedRuns $Runs
    Write-Host "Performance report: $report"
    $artifacts | Format-Table build, bytes, sha256 -AutoSize
}
finally {
    Pop-Location
    foreach ($path in @($transientTargets) | Sort-Object Length -Descending -Unique) {
        Remove-TransientTarget -Path $path
    }
}
