function Assert-ExactAdbTarget {
    $deviceRows = & $adb devices 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "adb devices failed:`n$($deviceRows -join [Environment]::NewLine)"
    }
    $matches = @($deviceRows | ForEach-Object {
        $match = [regex]::Match($_, '^([^\s]+)\s+([^\s]+)')
        if ($match.Success -and $match.Groups[1].Value -eq $serial) {
            [pscustomobject]@{
                Serial = $match.Groups[1].Value
                State = $match.Groups[2].Value
            }
        }
    })
    if ($matches.Count -ne 1) {
        throw "Expected exactly one adb target with serial '$serial'; found $($matches.Count)."
    }
    if ($matches[0].State -ne "device") {
        throw "adb target '$serial' is not ready (state: $($matches[0].State))."
    }
    $targetState = ((Invoke-TargetAdb -AdbArguments @("get-state")) -join "").Trim()
    if ($targetState -ne "device") {
        throw "adb target '$serial' did not confirm the device state."
    }
}

function Read-CurrentAndroidUser {
    $output = ((Invoke-TargetAdb -AdbArguments @("shell", "am", "get-current-user")) -join "").Trim()
    if ($output -notmatch '^\d+$') {
        throw "Could not determine the current Android user on '$serial'."
    }
    return [int]$output
}

function Assert-TargetAndroidUser {
    $currentUser = Read-CurrentAndroidUser
    if ($currentUser -ne $targetUserId) {
        throw "Phone Control harness is bound to Android user $targetUserId, but user $currentUser is active."
    }
}

function Read-AndroidUserIds {
    $output = Invoke-TargetAdb -AdbArguments @("shell", "pm", "list", "users")
    $ids = @($output | ForEach-Object {
        $match = [regex]::Match($_, 'UserInfo\{(?<id>\d+):')
        if ($match.Success) {
            [int]$match.Groups["id"].Value
        }
    } | Sort-Object -Unique)
    if ($ids.Count -eq 0 -or $ids -notcontains $targetUserId) {
        throw "Could not verify Android user $targetUserId on '$serial'."
    }
    return $ids
}

function Test-PackageInstalledForUser {
    param(
        [Parameter(Mandatory = $true)][string]$PackageName,
        [Parameter(Mandatory = $true)][int]$UserId
    )

    $listedPackages = Invoke-TargetAdb -AdbArguments @(
        "shell", "pm", "list", "packages", "--user", "$UserId", $PackageName
    )
    $exactPackageLine = "package:$PackageName"
    return @($listedPackages | ForEach-Object { $_.Trim() }) -contains $exactPackageLine
}

function Assert-PackagesAbsentForAllUsers {
    param([Parameter(Mandatory = $true)][string[]]$PackageNames)

    $present = [System.Collections.Generic.List[string]]::new()
    foreach ($userId in Read-AndroidUserIds) {
        foreach ($packageName in $PackageNames) {
            if (Test-PackageInstalledForUser $packageName $userId) {
                $present.Add("$packageName (user $userId)")
            }
        }
    }
    if ($present.Count -gt 0) {
        throw "Phone Control debug/test package state is not clean: $($present -join ', ')."
    }
}

function Assert-PackageScopedToTargetUser {
    param([Parameter(Mandatory = $true)][string]$PackageName)

    $users = @(Read-AndroidUserIds)
    if (-not (Test-PackageInstalledForUser $PackageName $targetUserId)) {
        throw "Package '$PackageName' is not installed for Android user $targetUserId."
    }
    $unexpectedUsers = @($users | Where-Object {
        $_ -ne $targetUserId -and (Test-PackageInstalledForUser $PackageName $_)
    })
    if ($unexpectedUsers.Count -gt 0) {
        throw "Package '$PackageName' escaped Android user $targetUserId into user(s) $($unexpectedUsers -join ', ')."
    }
}

function Read-PackagePaths {
    param(
        [Parameter(Mandatory = $true)][string]$PackageName,
        [int]$UserId = $targetUserId
    )

    if (-not (Test-PackageInstalledForUser $PackageName $UserId)) {
        return @()
    }

    $lines = Invoke-TargetAdb -AdbArguments @(
        "shell", "pm", "path", "--user", "$UserId", $PackageName
    )
    $paths = @($lines | ForEach-Object { $_.Trim() } | Where-Object { $_ -like "package:*" })
    if ($paths.Count -eq 0) {
        throw "Package manager listed '$PackageName' but returned no installed paths."
    }
    return $paths
}

function Test-PackageInstalled {
    param([Parameter(Mandatory = $true)][string]$PackageName)

    $paths = @(Read-PackagePaths $PackageName)
    return $paths.Count -gt 0
}

function Read-PackageAttestation {
    param([Parameter(Mandatory = $true)][string]$PackageName)

    $installedUserIds = @(Read-AndroidUserIds | Where-Object {
        Test-PackageInstalledForUser $PackageName $_
    })
    $paths = @(Read-PackagePaths $PackageName | Sort-Object)
    if ($paths.Count -eq 0) {
        return [ordered]@{
            installed = $false
            user_ids = $installedUserIds
            version_code = $null
            version_name = $null
            last_update_time = $null
            files = @()
        }
    }

    $details = (Invoke-TargetAdb -AdbArguments @("shell", "dumpsys", "package", $PackageName)) -join "`n"
    $versionCode = [regex]::Match($details, '(?m)^\s*versionCode=(?<value>\d+)\b')
    $versionName = [regex]::Match($details, '(?m)^\s*versionName=(?<value>[^\r\n]*)$')
    $lastUpdate = [regex]::Match($details, '(?m)^\s*lastUpdateTime=(?<value>[^\r\n]*)$')
    if (-not $versionCode.Success -or -not $versionName.Success -or -not $lastUpdate.Success) {
        throw "Could not read stable package metadata for '$PackageName'."
    }

    $files = @($paths | ForEach-Object {
        $devicePath = $_.Substring("package:".Length)
        $hashOutput = (
            Invoke-TargetAdb -AdbArguments @("shell", "sha256sum", $devicePath)
        ) -join "`n"
        $hash = [regex]::Match($hashOutput, '^(?<value>[0-9a-fA-F]{64})\s')
        if (-not $hash.Success) {
            throw "Could not attest installed package file '$devicePath'."
        }
        [ordered]@{
            path = $_
            sha256 = $hash.Groups["value"].Value.ToLowerInvariant()
        }
    })
    return [ordered]@{
        installed = $true
        user_ids = $installedUserIds
        version_code = $versionCode.Groups["value"].Value
        version_name = $versionName.Groups["value"].Value.Trim()
        last_update_time = $lastUpdate.Groups["value"].Value.Trim()
        files = $files
    }
}

function Normalize-SettingValue {
    param([AllowNull()][string]$Value)

    if (-not $Value -or $Value -eq "null") {
        return "null"
    }
    return $Value.TrimEnd("`r", "`n")
}

function Read-AndroidSetting {
    param(
        [ValidateSet("secure", "global")][string]$Namespace,
        [string]$Key
    )

    $value = (
        Invoke-TargetAdb -AdbArguments @(
            "shell", "settings", "--user", "$targetUserId", "get", $Namespace, $Key
        )
    ) -join "`n"
    return Normalize-SettingValue $value
}

function Restore-AndroidSetting {
    param(
        [ValidateSet("secure", "global")][string]$Namespace,
        [string]$Key,
        [AllowNull()][string]$Value
    )

    Assert-TargetAndroidUser
    if ((Normalize-SettingValue $Value) -eq "null") {
        Invoke-TargetAdb -AdbArguments @(
            "shell", "settings", "--user", "$targetUserId", "delete", $Namespace, $Key
        ) | Out-Null
    } else {
        Invoke-TargetAdb -AdbArguments @(
            "shell", "settings", "--user", "$targetUserId", "put", $Namespace, $Key, $Value
        ) | Out-Null
    }
    $restored = Read-AndroidSetting $Namespace $Key
    if ($restored -ne (Normalize-SettingValue $Value)) {
        throw "Failed to restore Android $Namespace setting '$Key'."
    }
}

function Enable-HarnessStayAwake {
    Assert-TargetAndroidUser
    Restore-AndroidSetting "global" "stay_on_while_plugged_in" "7"
    Invoke-TargetAdb -AdbArguments @("shell", "input", "keyevent", "KEYCODE_WAKEUP") | Out-Null

    $powerState = (Invoke-TargetAdb -AdbArguments @("shell", "dumpsys", "power")) -join "`n"
    if ($powerState -notmatch '(?m)^\s*mWakefulness=Awake\s*$') {
        throw "Phone Control device did not become awake for the visible probe session."
    }

    $windowPolicy = (Invoke-TargetAdb -AdbArguments @("shell", "dumpsys", "window", "policy")) -join "`n"
    $keyguard = [regex]::Match(
        $windowPolicy,
        '(?ms)^\s*KeyguardServiceDelegate\s*$.*?^\s*showing=(?<showing>true|false)\s*$'
    )
    if (-not $keyguard.Success -or $keyguard.Groups["showing"].Value -ne "false") {
        throw "Phone Control visible probes require the selected device to be unlocked."
    }
    Write-Host "Kept $serial awake for the journaled Phone Control run"
}

function Restore-HarnessPowerState {
    if (-not $runState.Contains("power")) {
        return
    }
    $powerState = $runState["power"]
    if (-not $powerState["captured"]) {
        return
    }

    Assert-TargetAndroidUser
    Restore-AndroidSetting `
        "global" `
        "stay_on_while_plugged_in" `
        ([string]$powerState["stay_on_while_plugged_in"])
    $powerState["captured"] = $false
    Write-RecoveryState
    Write-Host "Restored power state on $serial"
}

function Read-ForegroundState {
    $activities = (Invoke-TargetAdb -AdbArguments @("shell", "dumpsys", "activity", "activities")) -join "`n"
    $match = [regex]::Match(
        $activities,
        '(?m)mResumedActivity: ActivityRecord\{[^\r\n}]*\s(?<component>[A-Za-z0-9._$]+/[A-Za-z0-9._$]+)\s+t(?<task>\d+)'
    )
    if (-not $match.Success) {
        return [ordered]@{ component = $null; package = $null }
    }
    $component = $match.Groups["component"].Value
    return [ordered]@{
        component = $component
        package = $component.Split('/')[0]
    }
}

function Read-HomePackage {
    $resolved = (
        Invoke-TargetAdb -AdbArguments @(
            "shell", "cmd", "package", "resolve-activity", "--user", "$targetUserId", "--brief",
            "-a", "android.intent.action.MAIN", "-c", "android.intent.category.HOME"
        )
    ) -join "`n"
    $match = [regex]::Match($resolved, '(?<component>[A-Za-z0-9._$]+/[A-Za-z0-9._$]+)')
    if (-not $match.Success) {
        return $null
    }
    return $match.Groups["component"].Value.Split('/')[0]
}

function Restore-ForegroundState {
    param([System.Collections.IDictionary]$Foreground)

    Assert-TargetAndroidUser
    $component = [string]$Foreground["component"]
    $packageName = [string]$Foreground["package"]
    if (-not $component -or -not $packageName) {
        return
    }
    $current = Read-ForegroundState
    if ($current["component"] -eq $component) {
        return
    }
    $homePackage = Read-HomePackage
    if ($packageName -eq $homePackage) {
        Invoke-TargetAdb -AdbArguments @("shell", "input", "keyevent", "KEYCODE_HOME") | Out-Null
    } else {
        Invoke-TargetAdb -AdbArguments @(
            "shell", "am", "start", "--user", "$targetUserId", "-W", "-n", $component
        ) | Out-Null
    }
    for ($attempt = 0; $attempt -lt 20; $attempt += 1) {
        Start-Sleep -Milliseconds 100
        $current = Read-ForegroundState
        if ($current["package"] -eq $packageName) {
            return
        }
    }
    throw "Could not verify restoration of the original foreground package."
}

function Write-RecoveryState {
    if (-not $runState -or -not $recoveryStatePath) {
        return
    }
    $temporary = "$recoveryStatePath.tmp"
    $json = $runState | ConvertTo-Json -Depth 12
    Set-Content -LiteralPath $temporary -Value $json -Encoding utf8NoBOM
    Move-Item -LiteralPath $temporary -Destination $recoveryStatePath -Force
}

function Set-PackageOwnership {
    param(
        [ValidateSet("app", "test")][string]$PackageKind,
        [bool]$Owned
    )

    $runState["packages"]["${PackageKind}_owned"] = $Owned
    Write-RecoveryState
}

function Remove-OwnedInstrumentationPackage {
    if ($runState["packages"]["test_owned"]) {
        Assert-TargetAndroidUser
        Invoke-OptionalTargetAdb -AdbArguments @("uninstall", "--user", "$targetUserId", $testPackage)
        if (Test-PackageInstalled $testPackage) {
            throw "Could not remove the instrumentation package installed by this run."
        }
        Set-PackageOwnership "test" $false
    }
}

function Remove-OwnedTestPackages {
    Remove-PhoneControlProbeReceipts
    Remove-OwnedInstrumentationPackage
    if ($runState["packages"]["app_owned"]) {
        Assert-TargetAndroidUser
        Invoke-OptionalTargetAdb -AdbArguments @("uninstall", "--user", "$targetUserId", $appPackage)
        if (Test-PackageInstalled $appPackage) {
            throw "Could not remove the debug app package installed by this run."
        }
        Set-PackageOwnership "app" $false
    }
    Assert-PackagesAbsentForAllUsers @($appPackage, $testPackage)
}

function Assert-ReleasePackageUnchanged {
    if ($runState["packages"].Contains("release_attestation")) {
        $before = $runState["packages"]["release_attestation"] | ConvertTo-Json -Depth 8 -Compress
        $after = Read-PackageAttestation $releasePackage | ConvertTo-Json -Depth 8 -Compress
    } else {
        $before = @($runState["packages"]["release_paths"]) -join "`n"
        $after = @(Read-PackagePaths $releasePackage) -join "`n"
    }
    if ($before -ne $after) {
        throw "The release app package changed during the Phone Control test run."
    }
}
