param(
    [Parameter(Mandatory = $true)]
    [string]$Serial,
    [ValidateSet("Release", "Debug")]
    [string]$Variant = "Release",
    [string]$OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$packageName = if ($Variant -eq "Debug") {
    "dev.screengoated.toolbox.mobile.debug"
} else {
    "dev.screengoated.toolbox.mobile"
}
$candidateAdbPaths = @(
    $(if ($env:ANDROID_HOME) { Join-Path $env:ANDROID_HOME "platform-tools\adb.exe" }),
    $(if ($env:ANDROID_SDK_ROOT) { Join-Path $env:ANDROID_SDK_ROOT "platform-tools\adb.exe" }),
    $(if ($env:USERPROFILE) { Join-Path $env:USERPROFILE "android-sdk\platform-tools\adb.exe" }),
    $(if ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA "Android\Sdk\platform-tools\adb.exe" })
) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
$adb = $candidateAdbPaths | Select-Object -First 1
if (-not $adb) {
    throw "adb.exe was not found in ANDROID_HOME, ANDROID_SDK_ROOT, or standard SDK paths."
}

function Invoke-TargetAdb {
    param([Parameter(Mandatory = $true)][string[]]$AdbArguments)

    $output = & $adb -s $Serial @AdbArguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "adb -s $Serial $($AdbArguments -join ' ') failed:`n$($output -join [Environment]::NewLine)"
    }
    return @($output)
}

if (((Invoke-TargetAdb -AdbArguments @("get-state")) -join "").Trim() -ne "device") {
    throw "Android target $Serial is not in the device state."
}
$targetUser = ((Invoke-TargetAdb -AdbArguments @("shell", "am", "get-current-user")) -join "").Trim()
if ($targetUser -ne "0") {
    throw "Phone Control diagnostics are restricted to Android user 0; $Serial reports user $targetUser."
}
$installed = (Invoke-TargetAdb -AdbArguments @(
    "shell", "pm", "list", "packages", "--user", "0", $packageName
)) -join "`n"
if ($installed -notmatch "(?m)^package:$([regex]::Escape($packageName))$") {
    throw "$packageName is not installed for Android user 0 on $Serial."
}

if (-not $OutputDirectory) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $OutputDirectory = Join-Path $PSScriptRoot "phone-control-diagnostics-$stamp"
}
$resolvedOutput = [System.IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $resolvedOutput -Force | Out-Null

$remoteDirectory = "/sdcard/Android/data/$packageName/files/phone-control-diagnostics"
$remoteListing = & $adb -s $Serial shell ls -1 $remoteDirectory 2>$null
if ($LASTEXITCODE -eq 0) {
    foreach ($fileName in @("events.jsonl", "events.previous.jsonl")) {
        if (@($remoteListing) -contains $fileName) {
            $remotePath = "$remoteDirectory/$fileName"
            & $adb -s $Serial pull $remotePath (Join-Path $resolvedOutput $fileName) | Out-Null
            if ($LASTEXITCODE -ne 0) {
                throw "Failed to pull $remotePath from $Serial."
            }
        }
    }
}

$logcat = Invoke-TargetAdb -AdbArguments @("logcat", "-d", "-v", "threadtime")
@($logcat | Select-String -SimpleMatch "SGTPhoneControl" | ForEach-Object { $_.Line }) |
    Set-Content -LiteralPath (Join-Path $resolvedOutput "logcat.txt") -Encoding utf8

[ordered]@{
    captured_at = (Get-Date).ToUniversalTime().ToString("o")
    serial = $Serial
    android_user = 0
    package = $packageName
    variant = $Variant.ToLowerInvariant()
} | ConvertTo-Json | Set-Content -LiteralPath (
    Join-Path $resolvedOutput "capture.json"
) -Encoding utf8

Write-Host "Phone Control diagnostics collected at $resolvedOutput"
