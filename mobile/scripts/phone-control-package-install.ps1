function Remove-VerifiedPhoneControlStagingDirectory {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Root
    )

    $fullPath = [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    $fullRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd('\', '/')
    $containedPrefix = $fullRoot + [System.IO.Path]::DirectorySeparatorChar
    if (-not $fullPath.StartsWith($containedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove uncontained Phone Control staging path '$fullPath'."
    }
    if (Test-Path -LiteralPath $fullPath) {
        Remove-Item -LiteralPath $fullPath -Recurse -Force
    }
}

function Expand-PhoneControlPlayArchive {
    param(
        [Parameter(Mandatory = $true)][string]$ArchivePath,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($ArchivePath)
    try {
        $seenNames = [System.Collections.Generic.HashSet[string]]::new(
            [System.StringComparer]::OrdinalIgnoreCase
        )
        $files = [System.Collections.Generic.List[System.IO.FileInfo]]::new()
        foreach ($entry in $archive.Entries) {
            if (-not $entry.FullName.StartsWith("splits/", [System.StringComparison]::Ordinal)) {
                continue
            }
            $match = [regex]::Match(
                $entry.FullName,
                '^splits/(?<name>[A-Za-z0-9_.-]+\.apk)$',
                [System.Text.RegularExpressions.RegexOptions]::CultureInvariant
            )
            if (-not $match.Success) {
                throw "Unsafe or unsupported Play split archive entry '$($entry.FullName)'."
            }
            $fileName = $match.Groups["name"].Value
            if (-not $seenNames.Add($fileName)) {
                throw "Duplicate Play split archive entry '$fileName'."
            }
            $outputPath = Join-Path $Destination $fileName
            $input = $entry.Open()
            $output = [System.IO.File]::Open(
                $outputPath,
                [System.IO.FileMode]::CreateNew,
                [System.IO.FileAccess]::Write,
                [System.IO.FileShare]::None
            )
            try {
                $input.CopyTo($output)
            } finally {
                $output.Dispose()
                $input.Dispose()
            }
            $files.Add((Get-Item -LiteralPath $outputPath))
        }
        if ($files.Count -eq 0) {
            throw "Play local-testing archive contains no APK splits."
        }
        return @($files)
    } finally {
        $archive.Dispose()
    }
}

function Install-PlayDebugForTargetUser {
    $archivePath = Join-Path $mobileRoot (
        "androidApp\build\outputs\local-testing\playDebug\$safeSerial\androidApp-play-debug.apks"
    )
    if (-not (Test-Path -LiteralPath $archivePath -PathType Leaf)) {
        throw "Play local-testing APK archive is missing after assembly."
    }

    $stagingRoot = Join-Path $mobileRoot "build\phone-control-play-install"
    New-Item -ItemType Directory -Path $stagingRoot -Force | Out-Null
    $stagingDirectory = Join-Path $stagingRoot "$safeSerial-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Path $stagingDirectory | Out-Null
    try {
        $splitFiles = @(Expand-PhoneControlPlayArchive $archivePath $stagingDirectory)
        $baseFiles = @($splitFiles | Where-Object { $_.Name -like "base-*.apk" })
        $deferredFiles = @($splitFiles | Where-Object { $_.Name -notlike "base-*.apk" })
        $localTestingFiles = @(
            $splitFiles | Where-Object { $_.Name -ne "base-master.apk" }
        )
        if (($baseFiles.Name -notcontains "base-master.apk") -or $deferredFiles.Count -eq 0) {
            throw "Play archive does not contain the expected base and deferred feature split sets."
        }

        Assert-TargetAndroidUser
        $installArguments = @("install-multiple", "--user", "$targetUserId") + @(
            $baseFiles | Sort-Object Name | ForEach-Object { $_.FullName }
        )
        Invoke-TargetAdb -AdbArguments $installArguments | Write-Host
        Assert-PackageScopedToTargetUser $appPackage

        $remoteDirectory = "/sdcard/Android/data/$appPackage/files/local_testing"
        Assert-TargetAndroidUser
        Invoke-TargetAdb -AdbArguments @("shell", "rm", "-rf", $remoteDirectory) | Out-Null
        Invoke-TargetAdb -AdbArguments @("shell", "mkdir", "-p", $remoteDirectory) | Out-Null
        foreach ($file in $localTestingFiles | Sort-Object Name) {
            Assert-TargetAndroidUser
            $remotePath = "$remoteDirectory/$($file.Name)"
            Invoke-TargetAdb -AdbArguments @("push", $file.FullName, $remotePath) | Out-Null
            $remoteSize = ((
                Invoke-TargetAdb -AdbArguments @("shell", "stat", "-c", "%s", $remotePath)
            ) -join "").Trim()
            if ($remoteSize -notmatch '^\d+$' -or [int64]$remoteSize -ne $file.Length) {
                throw "Play local-testing split '$($file.Name)' did not verify after transfer."
            }
        }
        Assert-TargetAndroidUser
        Invoke-TargetAdb -AdbArguments @("shell", "chmod", "775", $remoteDirectory) | Out-Null
        Assert-TargetAndroidUser
        Invoke-TargetAdb -AdbArguments @(
            "shell", "run-as", $appPackage, "rm", "-rf", "files/splitcompat"
        ) | Out-Null
        Assert-PackageScopedToTargetUser $appPackage
    } finally {
        Remove-VerifiedPhoneControlStagingDirectory $stagingDirectory $stagingRoot
    }
}

function Install-FlavorDebugApp {
    param([ValidateSet("full", "play")][string]$VariantFlavor)

    $displayFlavor = (Get-Culture).TextInfo.ToTitleCase($VariantFlavor)
    Reset-TestPackages
    Assert-PackagesAbsentForAllUsers @($appPackage, $testPackage)
    Write-Host "Installing clean Phone Control $displayFlavor debug for Android user $targetUserId on $serial"
    # Journal before dispatch so interrupted recovery may remove only user-zero state.
    Set-PackageOwnership "app" $true
    if ($VariantFlavor -eq "play") {
        Install-PlayDebugForTargetUser
    } else {
        $appApk = Join-Path $mobileRoot "androidApp\build\outputs\apk\full\debug\androidApp-full-debug.apk"
        if (-not (Test-Path -LiteralPath $appApk -PathType Leaf)) {
            throw "Full app APK is missing after assembly."
        }
        Assert-TargetAndroidUser
        Invoke-TargetAdb -AdbArguments @(
            "install", "--user", "$targetUserId", $appApk
        ) | Write-Host
        Assert-PackageScopedToTargetUser $appPackage
    }
    if (Test-PackageInstalled $testPackage) {
        throw "Phone Control instrumentation appeared during debug-app installation."
    }
}
