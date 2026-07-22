param(
    [ValidateSet("Both", "Full", "Play")]
    [string]$Flavor = "Both",
    [string]$Serial = "emulator-5554",
    [switch]$AllowPhysicalDevice,
    [switch]$IncludeExternalSetupTests,
    [switch]$RetainDebugForProbes,
    [switch]$PrepareDebugForProbes,
    [switch]$RestoreOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$serial = $Serial
$targetUserId = 0
$appPackage = "dev.screengoated.toolbox.mobile.debug"
$testPackage = "$appPackage.test"
$releasePackage = "dev.screengoated.toolbox.mobile"
$accessibilityComponent = "$appPackage/dev.screengoated.toolbox.mobile.service.SgtAccessibilityService"
$mobileRoot = Split-Path -Parent $PSScriptRoot
$gradle = Join-Path $mobileRoot "gradlew.bat"
$candidateAdbPaths = @(
    $(if ($env:ANDROID_HOME) { Join-Path $env:ANDROID_HOME "platform-tools\adb.exe" }),
    $(if ($env:ANDROID_SDK_ROOT) { Join-Path $env:ANDROID_SDK_ROOT "platform-tools\adb.exe" }),
    $(if ($env:USERPROFILE) { Join-Path $env:USERPROFILE "android-sdk\platform-tools\adb.exe" }),
    $(if ($env:LOCALAPPDATA) { Join-Path $env:LOCALAPPDATA "Android\Sdk\platform-tools\adb.exe" })
) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
$adb = $candidateAdbPaths | Select-Object -First 1
if (-not $adb) {
    throw "adb.exe was not found in ANDROID_HOME, ANDROID_SDK_ROOT, or the standard user SDK paths."
}

$runState = $null
$recoveryStatePath = $null
$stateLockPath = $null
$stateLockHandle = $null
$isPhysicalDevice = $false

function Invoke-TargetAdb {
    param([Parameter(Mandatory = $true)][string[]]$AdbArguments)

    $output = & $adb -s $serial @AdbArguments 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "adb -s $serial $($AdbArguments -join ' ') failed:`n$($output -join [Environment]::NewLine)"
    }
    return $output
}

function Invoke-OptionalTargetAdb {
    param([Parameter(Mandatory = $true)][string[]]$AdbArguments)

    & $adb -s $serial @AdbArguments 2>$null | Out-Null
}

. (Join-Path $PSScriptRoot "phone-control-device-state.ps1")
. (Join-Path $PSScriptRoot "phone-control-package-install.ps1")
. (Join-Path $PSScriptRoot "phone-control-external-setup.ps1")

function Remove-PhoneControlProbeReceipts {
    Assert-TargetAndroidUser
    Invoke-OptionalTargetAdb -AdbArguments @(
        "shell", "run-as", $appPackage, "rm", "-rf", "no_backup/phone-control-probes"
    )
}

function Reset-TestPackages {
    Remove-OwnedTestPackages
}

function Get-FlavorBuildTasks {
    param(
        [string[]]$VariantFlavors,
        [bool]$IncludeInstrumentation
    )

    foreach ($variantFlavor in $VariantFlavors) {
        $displayFlavor = (Get-Culture).TextInfo.ToTitleCase($variantFlavor)
        if ($variantFlavor -eq "play") {
            ":androidApp:buildPlayDebugLocalTestingApks"
        } else {
            ":androidApp:assemble${displayFlavor}Debug"
        }
        if ($IncludeInstrumentation) {
            ":androidApp:assemble${displayFlavor}DebugAndroidTest"
        }
    }
}

function Read-SecureSetting {
    param([string]$Key)

    return Read-AndroidSetting "secure" $Key
}

function Restore-SecureSetting {
    param(
        [string]$Key,
        [string]$Value
    )

    Restore-AndroidSetting "secure" $Key $Value
}

function Normalize-SecureSetting {
    param([string]$Value)

    return Normalize-SettingValue $Value
}

function Restore-AccessibilityState {
    param(
        [string]$EnabledServices,
        [string]$AccessibilityEnabled
    )

    Restore-SecureSetting "enabled_accessibility_services" $EnabledServices
    Restore-SecureSetting "accessibility_enabled" $AccessibilityEnabled
    $restoredServices = Read-SecureSetting "enabled_accessibility_services"
    $restoredEnabled = Read-SecureSetting "accessibility_enabled"
    if ((Normalize-SecureSetting $restoredServices) -ne
        (Normalize-SecureSetting $EnabledServices) -or
        (Normalize-SecureSetting $restoredEnabled) -ne
        (Normalize-SecureSetting $AccessibilityEnabled)) {
        throw "Phone Control device harness did not restore the original Accessibility state."
    }
    Write-Host "Restored Accessibility state on $serial"
}

function Merge-EnabledAccessibilityService {
    param(
        [string]$EnabledServices,
        [string]$ComponentName
    )

    $services = @(
        if ((Normalize-SecureSetting $EnabledServices) -ne "null") {
            $EnabledServices -split ":" | Where-Object { $_ }
        }
    )
    $mergedServices = @($services) + @($ComponentName)
    return (($mergedServices | Select-Object -Unique) -join ":")
}

function Test-AccessibilityServiceBound {
    param(
        [string]$ActivityServicesOutput,
        [string]$ComponentName
    )

    $activeMatch = [regex]::Match(
        $ActivityServicesOutput,
        "(?ms)^\s*User $targetUserId active services:\s*(?<active>.*?)(?=^\s*Connection bindings to services:|\z)"
    )
    if (-not $activeMatch.Success) {
        return $false
    }
    $active = $activeMatch.Groups["active"].Value
    return $active.Contains($ComponentName) -and
        [regex]::IsMatch($active, "(?m)^\s*app=ProcessRecord\{")
}

function Test-TargetProcessRunning {
    param([string]$PackageName)

    $processes = (Invoke-TargetAdb -AdbArguments @("shell", "ps", "-A")) -join "`n"
    $processPattern = "(?m)\s" + [regex]::Escape($PackageName) + "(?::\S+)?\s*$"
    return [regex]::IsMatch($processes, $processPattern)
}

function Enable-AccessibilityAndAssertBinds {
    param(
        [string]$OriginalServices
    )

    $enabledServices = Merge-EnabledAccessibilityService $OriginalServices $accessibilityComponent
    Restore-SecureSetting "enabled_accessibility_services" $enabledServices
    Restore-SecureSetting "accessibility_enabled" "1"
    for ($attempt = 1; $attempt -le 50; $attempt += 1) {
        $activityServices = (
            Invoke-TargetAdb -AdbArguments @(
                "shell",
                "dumpsys",
                "activity",
                "services",
                $accessibilityComponent
            )
        ) -join "`n"
        if ((Test-AccessibilityServiceBound $activityServices $accessibilityComponent) -and
            (Test-TargetProcessRunning $appPackage)) {
            Write-Host "Verified bound SGT Accessibility service for Phone Control"
            return
        }
        Start-Sleep -Milliseconds 200
    }
    throw "SGT Accessibility service was enabled but did not become bound on $serial."
}

function Assert-AccessibilityServiceBinds {
    param(
        [string]$OriginalServices,
        [string]$OriginalAccessibilityEnabled
    )

    try {
        Enable-AccessibilityAndAssertBinds $OriginalServices
    } finally {
        Restore-AccessibilityState $OriginalServices $OriginalAccessibilityEnabled
    }
}

function Read-AppOpMode {
    param(
        [string]$PackageName,
        [string]$Operation
    )

    $arguments = @(
        "shell", "appops", "get", "--user", "$targetUserId", $PackageName, $Operation
    )
    $output = (Invoke-TargetAdb -AdbArguments $arguments) -join "`n"
    $match = [regex]::Match($output, "${Operation}:\s+(allow|deny|ignore|default|foreground)")
    if ($match.Success) {
        return $match.Groups[1].Value
    }
    return "default"
}

function Restore-AppOpMode {
    param(
        [string]$PackageName,
        [string]$Operation,
        [string]$Mode
    )

    Assert-TargetAndroidUser
    $arguments = @(
        "shell", "appops", "set", "--user", "$targetUserId", $PackageName, $Operation, $Mode
    )
    Invoke-TargetAdb -AdbArguments $arguments | Out-Null
}

function Run-FlavorTests {
    param(
        [ValidateSet("full", "play")][string]$VariantFlavor,
        [string]$OriginalServices,
        [string]$OriginalAccessibilityEnabled
    )

    $displayFlavor = (Get-Culture).TextInfo.ToTitleCase($VariantFlavor)
    $testApk = Join-Path $mobileRoot "androidApp\build\outputs\apk\androidTest\$VariantFlavor\debug\androidApp-$VariantFlavor-debug-androidTest.apk"
    if (-not (Test-Path -LiteralPath $testApk)) {
        throw "$displayFlavor test APK is missing after assembly."
    }

    Install-FlavorDebugApp $VariantFlavor
    Set-PackageOwnership "test" $true
    Assert-TargetAndroidUser
    Invoke-TargetAdb -AdbArguments @(
        "install", "--user", "$targetUserId", $testApk
    ) | Write-Host
    Assert-PackageScopedToTargetUser $testPackage
    Assert-AccessibilityServiceBinds $OriginalServices $OriginalAccessibilityEnabled
    $originalOverlayMode = Read-AppOpMode $appPackage "SYSTEM_ALERT_WINDOW"
    $runState["overlay"]["captured"] = $true
    $runState["overlay"]["mode"] = $originalOverlayMode
    Write-RecoveryState
    $expectShizukuRoute = $IncludeExternalSetupTests -and
        -not (Test-PackageInstalledForUser "moe.shizuku.privileged.api" $targetUserId)
    $shizukuRouteStamp = if ($expectShizukuRoute) {
        Read-ShizukuInstallRouteTaskStamp
    } else {
        0L
    }

    try {
        # Target instrumentation can force-stop a target-hosted AccessibilityService.
        # Keep the bind oracle in the host process; settings alone do not prove readiness.
        $classes = @(
            "dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLauncherSmokeTest",
            "dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlPackageCapabilityTest",
            "dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlOverlayExclusionTest"
        )
        if ($IncludeExternalSetupTests) {
            $classes +=
                "dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlShizukuSetupDeviceTest"
        }
        if ($VariantFlavor -eq "play") {
            $classes +=
                "dev.screengoated.toolbox.mobile.phonecontrol.PlayPhoneControlDetectorDeliveryTest"
        } else {
            $classes +=
                "dev.screengoated.toolbox.mobile.phonecontrol.FullPhoneControlDetectorDeliveryTest"
        }
        $classes = $classes -join ","
        $instrumentArguments = @(
            "shell",
            "am",
            "instrument",
            "--user",
            "$targetUserId",
            "-w",
            "-r",
            "-e",
            "class",
            $classes,
            "dev.screengoated.toolbox.mobile.debug.test/androidx.test.runner.AndroidJUnitRunner"
        )
        $result = Invoke-TargetAdb -AdbArguments $instrumentArguments
        $result | Write-Host
        $joined = $result -join "`n"
        if ($joined -match "FAILURES!!!|INSTRUMENTATION_FAILED|Process crashed" -or
            $joined -notmatch "OK \([1-9][0-9]* tests?\)") {
            throw "Phone Control $displayFlavor instrumentation did not complete successfully."
        }
        if ($expectShizukuRoute) {
            Assert-NewShizukuInstallRouteTask $shizukuRouteStamp
        }
    } finally {
        Remove-PhoneControlProbeReceipts
        Restore-AppOpMode $appPackage "SYSTEM_ALERT_WINDOW" $originalOverlayMode
        $restoredOverlayMode = Read-AppOpMode $appPackage "SYSTEM_ALERT_WINDOW"
        if ($restoredOverlayMode -ne $originalOverlayMode) {
            throw "Phone Control device harness did not restore the original overlay app-op."
        }
        $runState["overlay"]["captured"] = $false
        Write-RecoveryState
        Write-Host "Restored overlay app-op for Phone Control $displayFlavor"
    }
}

function Invoke-RunRestoration {
    param([switch]$RemoveRecoveryStateOnSuccess)

    if (-not $runState) {
        return
    }
    Assert-TargetAndroidUser
    $failures = [System.Collections.Generic.List[string]]::new()
    $attestationFailures = [System.Collections.Generic.List[string]]::new()
    try {
        Restore-AccessibilityState `
            ([string]$runState["accessibility"]["enabled_services"]) `
            ([string]$runState["accessibility"]["enabled"])
    } catch {
        $failures.Add("Accessibility: $($_.Exception.Message)")
    }
    try {
        if ($runState["overlay"]["captured"] -and (Test-PackageInstalled $appPackage)) {
            $overlayMode = [string]$runState["overlay"]["mode"]
            Restore-AppOpMode $appPackage "SYSTEM_ALERT_WINDOW" $overlayMode
            if ((Read-AppOpMode $appPackage "SYSTEM_ALERT_WINDOW") -ne $overlayMode) {
                throw "overlay app-op verification failed"
            }
            $runState["overlay"]["captured"] = $false
            Write-RecoveryState
        }
    } catch {
        $failures.Add("Overlay: $($_.Exception.Message)")
    }
    try {
        Remove-OwnedTestPackages
    } catch {
        $failures.Add("Packages: $($_.Exception.Message)")
    }
    try {
        Restore-ForegroundState $runState["foreground"]
    } catch {
        $attestationFailures.Add("Foreground: $($_.Exception.Message)")
    }
    try {
        Restore-HarnessPowerState
    } catch {
        $failures.Add("Power: $($_.Exception.Message)")
    }
    try {
        Assert-ReleasePackageUnchanged
    } catch {
        $attestationFailures.Add("Release package: $($_.Exception.Message)")
    }
    if ($failures.Count -gt 0) {
        Write-RecoveryState
        throw "Phone Control device restoration was incomplete:`n- $($failures -join "`n- ")"
    }
    if ($RemoveRecoveryStateOnSuccess -and (Test-Path -LiteralPath $recoveryStatePath)) {
        Remove-Item -LiteralPath $recoveryStatePath -Force
    }
    if ($attestationFailures.Count -gt 0) {
        throw "Phone Control post-run attestation failed after recoverable state was restored:`n- $($attestationFailures -join "`n- ")"
    }
}

function Restore-StaleRecoveryState {
    if (-not (Test-Path -LiteralPath $recoveryStatePath)) {
        return $false
    }
    try {
        $savedState = Get-Content -Raw -LiteralPath $recoveryStatePath | ConvertFrom-Json -AsHashtable
    } catch {
        throw "Recovery state '$recoveryStatePath' is unreadable; refusing to mutate the device."
    }
    $currentFingerprint = ((Invoke-TargetAdb -AdbArguments @("shell", "getprop", "ro.build.fingerprint")) -join "").Trim()
    $savedUserId = if ($savedState["identity"].Contains("user_id")) {
        [int]$savedState["identity"]["user_id"]
    } else {
        $targetUserId
    }
    if ($savedState["identity"]["serial"] -ne $serial -or
        $savedState["identity"]["fingerprint"] -ne $currentFingerprint -or
        $savedUserId -ne $targetUserId -or
        (Read-CurrentAndroidUser) -ne $savedUserId) {
        throw "Recovery state does not belong to the exact connected device; refusing recovery."
    }
    $script:runState = $savedState
    Write-Host "Recovering interrupted Phone Control harness state for $serial"
    Invoke-RunRestoration -RemoveRecoveryStateOnSuccess
    $script:runState = $null
    return $true
}

try {
    Assert-ExactAdbTarget
    Assert-TargetAndroidUser
    $qemu = ((Invoke-TargetAdb -AdbArguments @("shell", "getprop", "ro.kernel.qemu")) -join "").Trim()
    $isPhysicalDevice = $qemu -ne "1"
    if ($isPhysicalDevice -and -not $AllowPhysicalDevice) {
        throw "Phone Control device harness requires a verified emulator unless -AllowPhysicalDevice is set."
    }

    $stateDirectory = Join-Path $mobileRoot "build\phone-control-device-state"
    New-Item -ItemType Directory -Path $stateDirectory -Force | Out-Null
    $safeSerial = $serial -replace '[^A-Za-z0-9._-]', '_'
    $recoveryStatePath = Join-Path $stateDirectory "$safeSerial.json"
    $stateLockPath = Join-Path $stateDirectory "$safeSerial.lock"
    try {
        $stateLockHandle = [System.IO.File]::Open(
            $stateLockPath,
            [System.IO.FileMode]::OpenOrCreate,
            [System.IO.FileAccess]::ReadWrite,
            [System.IO.FileShare]::None
        )
    } catch {
        throw "Another Phone Control harness owns target '$serial', or its lock cannot be opened."
    }
    $restoredPendingState = Restore-StaleRecoveryState
    if ($RestoreOnly) {
        if (-not $restoredPendingState) {
            Write-Host "No retained or interrupted Phone Control harness state exists for $serial"
        }
        return
    }

    if ($RetainDebugForProbes -and $PrepareDebugForProbes) {
        throw "-RetainDebugForProbes and -PrepareDebugForProbes are mutually exclusive."
    }
    if (($RetainDebugForProbes -or $PrepareDebugForProbes) -and $Flavor -eq "Both") {
        throw "A retained or prepared debug probe session requires exactly one flavor."
    }

    try {
        Assert-PackagesAbsentForAllUsers @($appPackage, $testPackage)
    } catch {
        $targetKind = if ($isPhysicalDevice) { "Physical target" } else { "Target" }
        throw "$targetKind '$serial' has Phone Control debug/test package state. Refusing to erase any user's data: $($_.Exception.Message)"
    }
    $flavors = switch ($Flavor) {
        "Full" { @("full") }
        "Play" { @("play") }
        default { @("full", "play") }
    }
    $tasks = @(Get-FlavorBuildTasks @($flavors) (-not $PrepareDebugForProbes))

    Push-Location $mobileRoot
    try {
        & $gradle @tasks "-PphoneControlDeviceSerial=$serial" --console=plain
        if ($LASTEXITCODE -ne 0) {
            $buildPurpose = if ($PrepareDebugForProbes) { "debug probe preparation" } else { "test APK assembly" }
            throw "Phone Control $buildPurpose failed."
        }
    } finally {
        Pop-Location
    }

    # BundleTool may read the exact device spec while building Play splits, but build time
    # still does not own mutable phone state. Re-check and capture the restorable baseline
    # immediately before the first device mutation so concurrent user state stays theirs.
    Assert-TargetAndroidUser
    Assert-PackagesAbsentForAllUsers @($appPackage, $testPackage)
    $fingerprint = ((Invoke-TargetAdb -AdbArguments @("shell", "getprop", "ro.build.fingerprint")) -join "").Trim()
    $originalForeground = Read-ForegroundState
    $originalServices = Read-SecureSetting "enabled_accessibility_services"
    $originalAccessibilityEnabled = Read-SecureSetting "accessibility_enabled"
    $originalStayAwake = Read-AndroidSetting "global" "stay_on_while_plugged_in"
    $runState = [ordered]@{
        schema_version = 4
        identity = [ordered]@{
            serial = $serial
            fingerprint = $fingerprint
            physical = $isPhysicalDevice
            user_id = $targetUserId
            observed_user_ids = @(Read-AndroidUserIds)
        }
        accessibility = [ordered]@{
            enabled_services = $originalServices
            enabled = $originalAccessibilityEnabled
        }
        foreground = $originalForeground
        power = [ordered]@{
            captured = $true
            stay_on_while_plugged_in = $originalStayAwake
        }
        overlay = [ordered]@{
            captured = $false
            mode = $null
        }
        packages = [ordered]@{
            app_preexisting = $false
            test_preexisting = $false
            app_owned = $false
            test_owned = $false
            release_attestation = Read-PackageAttestation $releasePackage
        }
    }
    Write-RecoveryState

    $retainedProbeSession = $false
    try {
        Enable-HarnessStayAwake
        if ($PrepareDebugForProbes) {
            $variantFlavor = @($flavors)[0]
            Install-FlavorDebugApp $variantFlavor
            Enable-AccessibilityAndAssertBinds $originalServices
            Assert-ReleasePackageUnchanged
            $retainedProbeSession = $true
            Write-Host "Prepared journaled Phone Control $Flavor debug probe session on $serial"
            Write-Host "No instrumentation or acceptance tests were run."
            Write-Host "Restore with: .\mobile\scripts\run-phone-control-tests.ps1 -Serial $serial -AllowPhysicalDevice -RestoreOnly"
        } else {
            foreach ($variantFlavor in $flavors) {
                try {
                    Run-FlavorTests $variantFlavor $originalServices $originalAccessibilityEnabled
                } finally {
                    Restore-AccessibilityState $originalServices $originalAccessibilityEnabled
                }
            }
            if ($RetainDebugForProbes) {
                Remove-OwnedInstrumentationPackage
                Enable-AccessibilityAndAssertBinds $originalServices
                Assert-ReleasePackageUnchanged
                $retainedProbeSession = $true
                Write-Host "Retained journaled Phone Control $Flavor debug session on $serial"
                Write-Host "Restore with: .\mobile\scripts\run-phone-control-tests.ps1 -Serial $serial -AllowPhysicalDevice -RestoreOnly"
            }
        }
    } finally {
        if (-not $retainedProbeSession) {
            Invoke-RunRestoration -RemoveRecoveryStateOnSuccess
        }
    }
} finally {
    if ($stateLockHandle) {
        $stateLockHandle.Dispose()
    }
    if ($stateLockPath -and (Test-Path -LiteralPath $stateLockPath)) {
        Remove-Item -LiteralPath $stateLockPath -Force -ErrorAction SilentlyContinue
    }
}
