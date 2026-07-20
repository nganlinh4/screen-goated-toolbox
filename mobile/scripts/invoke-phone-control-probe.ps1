param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$Tool,
    [string]$ArgumentsJson = "{}",
    [string]$Serial = "emulator-5554",
    [switch]$AllowPhysicalDevice,
    [switch]$AllowMutation,
    [ValidateRange(1, 120)]
    [int]$TimeoutSeconds = 20
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$appPackage = "dev.screengoated.toolbox.mobile.debug"
$receiver = "$appPackage/dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlDebugProbeReceiver"
$action = "dev.screengoated.toolbox.mobile.debug.PHONE_CONTROL_PROBE"
$cancelAction = "dev.screengoated.toolbox.mobile.debug.PHONE_CONTROL_PROBE_CANCEL"
$requestId = "probe_" + [Guid]::NewGuid().ToString("N")
$relativeResult = "no_backup/phone-control-probes/$requestId.json"
$probeDispatchAttempted = $false
$receiptObserved = $false
$candidateAdbPaths = @(
    $(if ($env:ANDROID_HOME) { Join-Path $env:ANDROID_HOME "platform-tools\adb.exe" }),
    $(if ($env:ANDROID_SDK_ROOT) { Join-Path $env:ANDROID_SDK_ROOT "platform-tools\adb.exe" }),
    $(if ($env:USERPROFILE) { Join-Path $env:USERPROFILE "android-sdk\platform-tools\adb.exe" }),
    $(if ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA "Android\Sdk\platform-tools\adb.exe" })
) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
$adb = $candidateAdbPaths | Select-Object -First 1
if (-not $adb) {
    throw "adb.exe was not found in the configured or standard Android SDK paths."
}

function Invoke-TargetAdb {
    param(
        [Parameter(Mandatory = $true)][string[]]$AdbArguments,
        [switch]$AllowFailure
    )

    $output = & $adb -s $Serial @AdbArguments 2>&1
    if ($LASTEXITCODE -ne 0 -and -not $AllowFailure) {
        throw "adb -s $Serial $($AdbArguments -join ' ') failed:`n$($output -join [Environment]::NewLine)"
    }
    return [pscustomobject]@{
        ExitCode = $LASTEXITCODE
        Output = @($output)
    }
}

function Assert-ExactAdbTarget {
    $deviceRows = & $adb devices 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "adb devices failed:`n$($deviceRows -join [Environment]::NewLine)"
    }
    $matches = @($deviceRows | ForEach-Object {
        $match = [regex]::Match($_, '^([^\s]+)\s+([^\s]+)')
        if ($match.Success -and $match.Groups[1].Value -eq $Serial) {
            $match.Groups[2].Value
        }
    })
    if ($matches.Count -ne 1 -or $matches[0] -ne "device") {
        throw "Expected exactly one ready adb target with serial '$Serial'."
    }
    $confirmed = Invoke-TargetAdb -AdbArguments @("get-state")
    if ((($confirmed.Output -join "").Trim()) -ne "device") {
        throw "adb target '$Serial' did not confirm the device state."
    }
}

function Remove-ProbeReceipt {
    for ($attempt = 0; $attempt -lt 3; $attempt += 1) {
        Invoke-TargetAdb -AllowFailure -AdbArguments @(
            "shell", "run-as", $appPackage, "rm", "-f", $relativeResult
        ) | Out-Null
        Invoke-TargetAdb -AllowFailure -AdbArguments @(
            "shell", "run-as", $appPackage, "rm", "-f",
            "no_backup/phone-control-probes/.$requestId.tmp"
        ) | Out-Null
        Invoke-TargetAdb -AllowFailure -AdbArguments @(
            "shell", "run-as", $appPackage, "rmdir", "no_backup/phone-control-probes"
        ) | Out-Null
        if ($attempt -lt 2) {
            Start-Sleep -Milliseconds 100
        }
    }
    $remaining = Invoke-TargetAdb -AllowFailure -AdbArguments @(
        "shell", "run-as", $appPackage, "ls", $relativeResult
    )
    if ($remaining.ExitCode -eq 0) {
        Write-Warning "Probe receipt '$requestId' could not be removed; debug-host expiry cleanup remains armed."
    }
}

Assert-ExactAdbTarget
$isEmulator = ((Invoke-TargetAdb -AdbArguments @("shell", "getprop", "ro.kernel.qemu")).Output -join "").Trim()
if ($isEmulator -ne "1" -and -not $AllowPhysicalDevice) {
    throw "Phone Control probes require a verified emulator unless -AllowPhysicalDevice is set."
}
$packagePaths = (Invoke-TargetAdb -AdbArguments @("shell", "pm", "path", $appPackage)).Output
if (-not @($packagePaths | Where-Object { $_ -like "package:*" })) {
    throw "The Phone Control debug package is not installed on exact target '$Serial'."
}
$runAsCheck = Invoke-TargetAdb -AllowFailure -AdbArguments @(
    "shell", "run-as", $appPackage, "id"
)
if ($runAsCheck.ExitCode -ne 0) {
    throw "The installed Phone Control package is not a debuggable host for probes."
}

try {
    $arguments = $ArgumentsJson | ConvertFrom-Json -AsHashtable
    $canonicalArguments = $arguments | ConvertTo-Json -Compress -Depth 30
} catch {
    throw "ArgumentsJson must be a JSON object: $($_.Exception.Message)"
}
if ($arguments -isnot [System.Collections.IDictionary]) {
    throw "ArgumentsJson must be a JSON object."
}

$encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($canonicalArguments))
$allowMutationValue = if ($AllowMutation) { "true" } else { "false" }
Remove-ProbeReceipt
try {
    $probeDispatchAttempted = $true
    Invoke-TargetAdb -AdbArguments @(
        "shell", "am", "broadcast",
        "-a", $action,
        "-n", $receiver,
        "--es", "request_id", $requestId,
        "--es", "tool", $Tool,
        "--es", "arguments_b64", $encoded,
        "--ez", "allow_mutation", $allowMutationValue
    ) | Out-Null
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        $result = Invoke-TargetAdb -AllowFailure -AdbArguments @(
            "shell", "run-as", $appPackage, "cat", $relativeResult
        )
        if ($result.ExitCode -eq 0 -and $result.Output) {
            $json = ($result.Output -join "`n").Trim()
            $null = $json | ConvertFrom-Json
            $receiptObserved = $true
            Write-Output $json
            return
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)

    $effectWarning = if ($AllowMutation) {
        " The acknowledged mutation may have occurred; take a fresh observation before any further mutation or completion."
    } else {
        ""
    }
    throw "Phone Control probe '$requestId' did not produce a result within $TimeoutSeconds seconds.$effectWarning"
} finally {
    Invoke-TargetAdb -AllowFailure -AdbArguments @(
        "shell", "am", "broadcast",
        "-a", $cancelAction,
        "-n", $receiver,
        "--es", "request_id", $requestId
    ) | Out-Null
    Remove-ProbeReceipt
    if ($probeDispatchAttempted -and -not $receiptObserved -and $AllowMutation) {
        Write-Warning "The mutating probe produced no receipt. Its effect is unknown; reconcile with a fresh observation."
    }
}
