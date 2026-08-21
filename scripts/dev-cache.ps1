[CmdletBinding(SupportsShouldProcess)]
param(
    [ValidateSet("Status", "Prune", "Path")]
    [string]$Action = "Status",
    [ValidateSet("dev", "package")]
    [string]$Lane = "dev",
    [string]$CacheRoot,
    [ValidateRange(5, 200)]
    [int]$MaxGiB = 28,
    [ValidateRange(1, 365)]
    [int]$InactiveDays = 14,
    [ValidateSet("none", "dev", "package")]
    [string]$ProtectLane = "none",
    [switch]$Apply
)

$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))

function Resolve-CacheRoot {
    if (-not [string]::IsNullOrWhiteSpace($CacheRoot)) {
        return [IO.Path]::GetFullPath($CacheRoot)
    }
    if (-not [string]::IsNullOrWhiteSpace($env:SGT_DEV_CACHE_ROOT)) {
        return [IO.Path]::GetFullPath($env:SGT_DEV_CACHE_ROOT)
    }
    $localData = [Environment]::GetFolderPath(
        [Environment+SpecialFolder]::LocalApplicationData
    )
    return Join-Path $localData "SGT-Development\cache"
}

function Assert-SafeCacheRoot {
    param([string]$Root)

    $full = [IO.Path]::GetFullPath($Root).TrimEnd('\')
    $repo = $repoRoot.TrimEnd('\')
    if ($full.Length -lt 12 -or $full -eq $repo -or $full.StartsWith("$repo\")) {
        throw "Development cache must be an explicit directory outside the repository: $full"
    }
    if ([IO.Path]::GetPathRoot($full).TrimEnd('\') -eq $full) {
        throw "Development cache cannot be a drive root: $full"
    }
    return $full
}

function Get-TreeSize {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        return [int64]0
    }
    $total = [int64]0
    Get-ChildItem -LiteralPath $Path -File -Recurse -Force -ErrorAction SilentlyContinue |
        ForEach-Object { $total += $_.Length }
    return $total
}

function Get-NewestWriteTime {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return [DateTime]::MinValue
    }
    $newest = Get-ChildItem -LiteralPath $Path -File -Recurse -Force -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if ($newest) {
        return $newest.LastWriteTimeUtc
    }
    return (Get-Item -LiteralPath $Path).LastWriteTimeUtc
}

function Test-PathInUse {
    param([string]$Path)

    $needle = [IO.Path]::GetFullPath($Path).ToLowerInvariant()
    try {
        $processes = Get-CimInstance Win32_Process -ErrorAction Stop
        foreach ($process in $processes) {
            $command = [string]$process.CommandLine
            $executable = [string]$process.ExecutablePath
            if ($command.ToLowerInvariant().Contains($needle) -or
                $executable.ToLowerInvariant().StartsWith($needle)) {
                return $true
            }
        }
    }
    catch {
        Write-Warning "Could not inspect process command lines; cache deletion is disabled: $_"
        return $true
    }
    return $false
}

function Remove-CacheEntry {
    param(
        [string]$Path,
        [string]$Reason
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return $false
    }
    if (Test-PathInUse $Path) {
        Write-Host "KEEP (in use): $Path" -ForegroundColor Yellow
        return $false
    }
    if (-not $Apply) {
        Write-Host "WOULD REMOVE ($Reason): $Path" -ForegroundColor Yellow
        return $false
    }
    if ($PSCmdlet.ShouldProcess($Path, "Remove bounded SGT development cache entry")) {
        Remove-Item -LiteralPath $Path -Recurse -Force
        Write-Host "REMOVED ($Reason): $Path" -ForegroundColor Green
        return $true
    }
    return $false
}

$root = Assert-SafeCacheRoot (Resolve-CacheRoot)
$known = @{
    DevCargo = Join-Path $root "cargo\dev"
    PackageCargo = Join-Path $root "cargo\package"
    Packages = Join-Path $root "packages"
    Evidence = Join-Path $root "evidence"
    Staging = Join-Path $root "staging"
    Runtime = Join-Path $root "runtime"
}

if ($Action -eq "Path") {
    if ($Lane -eq "package") {
        Write-Output $known.PackageCargo
    }
    else {
        Write-Output $known.DevCargo
    }
    exit 0
}

New-Item -ItemType Directory -Path $root -Force | Out-Null
foreach ($path in $known.Values) {
    New-Item -ItemType Directory -Path $path -Force | Out-Null
}

if ($Action -eq "Status") {
    $rows = foreach ($entry in $known.GetEnumerator() | Sort-Object Key) {
        $bytes = Get-TreeSize $entry.Value
        [pscustomobject]@{
            Area = $entry.Key
            GiB = [math]::Round($bytes / 1GB, 2)
            LastWriteUtc = Get-NewestWriteTime $entry.Value
            Path = $entry.Value
        }
    }
    $rows | Format-Table -AutoSize
    $total = Get-TreeSize $root
    Write-Host ("Total: {0:N2} GiB / {1} GiB" -f ($total / 1GB), $MaxGiB)
    exit 0
}

$cutoff = [DateTime]::UtcNow.AddDays(-$InactiveDays)
$candidates = [System.Collections.Generic.List[object]]::new()

$jobRoot = Join-Path $known.Packages "jobs"
Get-ChildItem -LiteralPath $jobRoot -Directory -Force -ErrorAction SilentlyContinue |
    ForEach-Object {
        Get-ChildItem -LiteralPath $_.FullName -Directory -Force -ErrorAction SilentlyContinue
    } |
    ForEach-Object {
        $candidates.Add([pscustomobject]@{
            Path = $_.FullName
            LastWriteUtc = Get-NewestWriteTime $_.FullName
            Protected = $false
        })
    }

$releaseRoot = Join-Path $known.Packages "release"
Get-ChildItem -LiteralPath $releaseRoot -Directory -Force -ErrorAction SilentlyContinue |
    ForEach-Object {
        $candidates.Add([pscustomobject]@{
            Path = $_.FullName
            LastWriteUtc = Get-NewestWriteTime $_.FullName
            Protected = $ProtectLane -eq "package"
        })
    }

Get-ChildItem -LiteralPath $known.Evidence -Directory -Force -ErrorAction SilentlyContinue |
    ForEach-Object {
        $candidates.Add([pscustomobject]@{
            Path = $_.FullName
            LastWriteUtc = Get-NewestWriteTime $_.FullName
            Protected = $false
        })
    }

$stagingRuntime = Join-Path $known.Runtime "staging"
$stagingAppRunning = $null -ne (Get-Process -Name "screen-goated-toolbox" -ErrorAction SilentlyContinue)
if (Test-Path -LiteralPath $stagingRuntime) {
    $candidates.Add([pscustomobject]@{
        Path = $stagingRuntime
        LastWriteUtc = Get-NewestWriteTime $stagingRuntime
        Protected = $stagingAppRunning
    })
}

$candidates.Add([pscustomobject]@{
    Path = $known.PackageCargo
    LastWriteUtc = Get-NewestWriteTime $known.PackageCargo
    Protected = $ProtectLane -eq "package"
})
$candidates.Add([pscustomobject]@{
    Path = $known.DevCargo
    LastWriteUtc = Get-NewestWriteTime $known.DevCargo
    Protected = $ProtectLane -eq "dev"
})

foreach ($candidate in $candidates | Where-Object {
    -not $_.Protected -and $_.LastWriteUtc -lt $cutoff
} | Sort-Object LastWriteUtc) {
    Remove-CacheEntry $candidate.Path "inactive for at least $InactiveDays days" | Out-Null
}

$limit = [int64]$MaxGiB * 1GB
$total = Get-TreeSize $root
if ($total -gt $limit) {
    foreach ($candidate in $candidates | Where-Object {
        -not $_.Protected -and (Test-Path -LiteralPath $_.Path)
    } | Sort-Object LastWriteUtc) {
        if ($total -le $limit) {
            break
        }
        $candidateSize = Get-TreeSize $candidate.Path
        if (Remove-CacheEntry $candidate.Path "cache exceeds $MaxGiB GiB") {
            $total = [math]::Max([int64]0, $total - $candidateSize)
        }
    }
}

$mode = if ($Apply) { "after pruning" } else { "current (dry run)" }
Write-Host ("Cache {0}: {1:N2} GiB / {2} GiB at {3}" -f $mode, ((Get-TreeSize $root) / 1GB), $MaxGiB, $root)
