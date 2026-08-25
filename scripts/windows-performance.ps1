param(
    [ValidateSet("Compare", "TrainPgo")]
    [string]$Action = "Compare",
    [switch]$SkipBuild,
    [int]$Runs = 3,
    [int]$TimeoutSeconds = 75
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

if ($Runs -lt 1 -or $Runs -gt 20) {
    throw "Runs must be between 1 and 20."
}

function Invoke-CargoBuild {
    param(
        [Parameter(Mandatory = $true)][string]$Profile,
        [string]$TargetDirectory,
        [string]$EncodedRustFlags
    )

    $savedTarget = $env:CARGO_TARGET_DIR
    $savedFlags = $env:CARGO_ENCODED_RUSTFLAGS
    try {
        if ($TargetDirectory) { $env:CARGO_TARGET_DIR = $TargetDirectory }
        else { Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue }
        if ($EncodedRustFlags) { $env:CARGO_ENCODED_RUSTFLAGS = $EncodedRustFlags }
        else { Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue }
        cargo build --locked --profile $Profile --target $targetTriple `
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
        [Parameter(Mandatory = $true)][string]$Reason
    )

    $failureId = [guid]::NewGuid().ToString("N").Substring(0, 8)
    $failureRoot = Join-Path $performanceRoot "failures\$failureId"
    New-Item -ItemType Directory -Path $failureRoot -Force | Out-Null
    $sessionLog = Join-Path $StateRoot "local-app-data\SGT\logs\session.log"
    if (Test-Path -LiteralPath $sessionLog -PathType Leaf) {
        Copy-Item -LiteralPath $sessionLog -Destination (Join-Path $failureRoot "session.log")
    }
    [ordered]@{
        generatedAt = (Get-Date).ToUniversalTime().ToString("o")
        build = $Build
        smoke = $Smoke
        run = $Run
        reason = $Reason
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
                -Smoke $Flag.TrimStart("-") -Run $Run -Reason $reason
            throw "$Build $Flag $reason."
        }
        $timer.Stop()
        $process.Refresh()
        if ($process.ExitCode -ne 0) {
            Save-SmokeFailure -StateRoot $stateRoot -Build $Build `
                -Smoke $Flag.TrimStart("-") -Run $Run `
                -Reason "exit code $($process.ExitCode)"
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

Push-Location $repoRoot
try {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
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
        $pgoExe = Join-Path $optimizedTarget `
            "$targetTriple\release-balanced\screen-goated-toolbox.exe"
        $validation = Invoke-SmokeCorpus -Build "pgo" -Executable $pgoExe
        $candidateExe = Join-Path $lane "screen-goated-toolbox-pgo.exe"
        $retainedProfile = Join-Path $lane "merged.profdata"
        Copy-Item -LiteralPath $pgoExe -Destination $candidateExe
        Copy-Item -LiteralPath $mergedProfile -Destination $retainedProfile
        $artifact = Get-ArtifactRecord -Build "pgo" -Executable $candidateExe
        $contract = Get-Content -LiteralPath (Join-Path $PSScriptRoot "performance-contract.json") `
            -Raw | ConvertFrom-Json
        $maxPgoBytes = [long]($contract.windows.maxShippingBinaryBytes *
            $contract.windows.maxPgoSizeRatio)
        if ($artifact.bytes -gt $maxPgoBytes) {
            throw "PGO binary is $($artifact.bytes) bytes; contract allows $maxPgoBytes."
        }
        $payload = [ordered]@{
            schemaVersion = 1
            generatedAt = (Get-Date).ToUniversalTime().ToString("o")
            rustc = (rustc -vV) -join "`n"
            artifact = $artifact
            training = $training
            validation = $validation
            profile = $retainedProfile
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
        Invoke-CargoBuild -Profile release-perf -TargetDirectory $compareTarget
    }
    $artifactRoot = if ($compareTarget) { $compareTarget } else { Join-Path $repoRoot "target" }
    $compactExe = Join-Path $artifactRoot `
        "$targetTriple\release-compact\screen-goated-toolbox.exe"
    $perfExe = Join-Path $artifactRoot "$targetTriple\release-perf\screen-goated-toolbox.exe"
    foreach ($path in @($compactExe, $perfExe)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Missing comparison artifact: $path"
        }
    }
    $artifacts = @(
        Get-ArtifactRecord -Build "compact" -Executable $compactExe
        Get-ArtifactRecord -Build "perf" -Executable $perfExe
    )
    $measurements = @()
    $measurements += Invoke-SmokeCorpus -Build "compact" -Executable $compactExe
    $measurements += Invoke-SmokeCorpus -Build "perf" -Executable $perfExe
    $payload = [ordered]@{
        schemaVersion = 1
        generatedAt = (Get-Date).ToUniversalTime().ToString("o")
        rustc = (rustc -vV) -join "`n"
        artifacts = $artifacts
        measurements = $measurements
    }
    $report = Join-Path $performanceRoot "windows-$stamp.json"
    $payload | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $report -Encoding utf8
    $contract = Get-Content -LiteralPath (Join-Path $PSScriptRoot "performance-contract.json") `
        -Raw | ConvertFrom-Json
    $compact = $artifacts | Where-Object build -eq "compact"
    $perf = $artifacts | Where-Object build -eq "perf"
    if ($compact.bytes -gt $contract.windows.maxShippingBinaryBytes) {
        throw "Compact binary is $($compact.bytes) bytes; contract allows $($contract.windows.maxShippingBinaryBytes)."
    }
    $sizeRatio = $perf.bytes / $compact.bytes
    if ($sizeRatio -gt $contract.windows.maxPerfSizeRatio) {
        throw "Perf/compact size ratio $([Math]::Round($sizeRatio, 3)) exceeds contract."
    }
    foreach ($smoke in $contract.windows.requiredSmokes) {
        $compactSamples = @($measurements | Where-Object {
            $_.build -eq "compact" -and $_.smoke -eq $smoke
        } | Sort-Object elapsedMs)
        $perfSamples = @($measurements | Where-Object {
            $_.build -eq "perf" -and $_.smoke -eq $smoke
        } | Sort-Object elapsedMs)
        if ($compactSamples.Count -ne $Runs -or $perfSamples.Count -ne $Runs) {
            throw "Missing samples for $smoke."
        }
        $middle = [Math]::Floor($Runs / 2)
        $compactMedian = $compactSamples[$middle].elapsedMs
        $perfMedian = $perfSamples[$middle].elapsedMs
        if ($perfMedian -gt $compactMedian * $contract.windows.maxPerfLatencyRatio) {
            throw "$smoke perf median ${perfMedian}ms regressed against compact ${compactMedian}ms."
        }
    }
    Write-Host "Performance report: $report"
    $artifacts | Format-Table build, bytes, sha256 -AutoSize
}
finally {
    Pop-Location
    foreach ($path in @($transientTargets) | Sort-Object Length -Descending -Unique) {
        Remove-TransientTarget -Path $path
    }
}
