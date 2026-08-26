param(
    [ValidateSet("Result", "Status", "Realtime")]
    [string]$Smoke = "Result",
    [string]$Executable,
    [int]$TimeoutSeconds = 90
)

$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$cacheRoot = if ([string]::IsNullOrWhiteSpace($env:SGT_DEV_CACHE_ROOT)) {
    Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)) `
        "SGT-Development\cache"
}
else { [IO.Path]::GetFullPath($env:SGT_DEV_CACHE_ROOT) }
. (Join-Path $PSScriptRoot "source-fingerprint.ps1")

if (-not (Get-Command wpr -ErrorAction SilentlyContinue)) {
    throw "Windows Performance Recorder (wpr.exe) is required."
}
$principal = [Security.Principal.WindowsPrincipal]::new(
    [Security.Principal.WindowsIdentity]::GetCurrent()
)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "Windows ETW capture requires an elevated PowerShell session."
}
$traceRoot = Join-Path $cacheRoot ("performance\traces\{0}-{1}" -f `
    $Smoke.ToLowerInvariant(), (Get-Date -Format "yyyyMMdd-HHmmss"))
New-Item -ItemType Directory -Path $traceRoot -Force | Out-Null

if ([string]::IsNullOrWhiteSpace($Executable)) {
    $Executable = Join-Path $cacheRoot `
        "performance\ownership\windows-target\x86_64-pc-windows-msvc\release-compact\screen-goated-toolbox.exe"
}
$Executable = [IO.Path]::GetFullPath($Executable)
if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
    throw "Missing profiled executable: $Executable. Run scripts\binary-ownership.ps1 first."
}
$executableSha256 = (Get-FileHash $Executable -Algorithm SHA256).Hash.ToLowerInvariant()
$ownershipRoot = Join-Path $cacheRoot "performance\ownership"
$ownershipMatch = Get-ChildItem -LiteralPath $ownershipRoot -Filter "binary-ownership-*.json" `
    -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending | ForEach-Object {
        try {
            $candidate = Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json
            if ($candidate.windows.artifact.sha256 -eq $executableSha256) {
                [pscustomobject]@{ Path = $_.FullName; Payload = $candidate }
            }
        }
        catch { }
    } | Select-Object -First 1
if (-not $ownershipMatch) {
    throw "No ownership report matches executable SHA-256 $executableSha256."
}
$flag = @{
    Result = "--result-compositor-smoke"
    Status = "--status-compositor-smoke"
    Realtime = "--realtime-compositor-smoke"
}[$Smoke]
$trace = Join-Path $traceRoot "sgt-$($Smoke.ToLowerInvariant()).etl"
$stateRoot = Join-Path $traceRoot "runtime-state"
New-Item -ItemType Directory -Path $stateRoot -Force | Out-Null

$status = (& wpr -status 2>&1) -join "`n"
if ($status -match "recording is in progress") {
    throw "WPR already owns an active recording; refusing to disturb it."
}
$started = $false
$process = $null
$traceSaved = $false
$savedState = $env:SGT_RUNTIME_STATE_ROOT
$savedOffscreen = $env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN
try {
    & wpr -start GeneralProfile.light -start CPU.light -start GPU.light `
        -start DesktopComposition.light
    if ($LASTEXITCODE -ne 0) { throw "WPR could not start; an elevated shell may be required." }
    $started = $true
    $env:SGT_RUNTIME_STATE_ROOT = $stateRoot
    if ($Smoke -eq "Result") { $env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN = "1" }
    else { Remove-Item Env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN -ErrorAction SilentlyContinue }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $Executable -ArgumentList $flag -PassThru `
        -WindowStyle Hidden
    while (-not $process.HasExited -and $timer.Elapsed.TotalSeconds -lt $TimeoutSeconds) {
        Start-Sleep -Milliseconds 200
    }
    if (-not $process.HasExited) {
        Stop-Process -Id $process.Id -Force
        throw "$Smoke smoke timed out after $TimeoutSeconds seconds."
    }
    if ($process.ExitCode -ne 0) { throw "$Smoke smoke exited with $($process.ExitCode)." }
}
finally {
    if ($started) {
        & wpr -stop $trace "SGT $Smoke compositor profile" -skipPdbGen -compress
        $traceSaved = $LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $trace -PathType Leaf)
    }
    if ($null -eq $savedState) { Remove-Item Env:SGT_RUNTIME_STATE_ROOT -ErrorAction SilentlyContinue }
    else { $env:SGT_RUNTIME_STATE_ROOT = $savedState }
    if ($null -eq $savedOffscreen) {
        Remove-Item Env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN -ErrorAction SilentlyContinue
    }
    else { $env:SGT_RESULT_COMPOSITOR_ACCEPTANCE_OFFSCREEN = $savedOffscreen }
}
if (-not $traceSaved) { throw "WPR failed to save $trace" }

$report = [ordered]@{
    schemaVersion = 1
    generatedAt = (Get-Date).ToUniversalTime().ToString("o")
    profiledSourceFingerprint = $ownershipMatch.Payload.sourceFingerprint
    captureSourceFingerprint = Get-SgtSourceFingerprint -RepoRoot $repoRoot
    ownershipReport = $ownershipMatch.Path
    smoke = $Smoke
    executable = $Executable
    executableSha256 = $executableSha256
    trace = $trace
}
$report | ConvertTo-Json | Set-Content (Join-Path $traceRoot "trace.json") -Encoding utf8
Write-Host "Windows ETW trace: $trace"
