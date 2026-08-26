param(
    [string]$Serial,
    [switch]$SkipBuild,
    [switch]$SkipBenchmark,
    [switch]$AllowEmulatorDiagnostics
)

$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$mobileRoot = Join-Path $repoRoot "mobile"
$contract = Join-Path $PSScriptRoot "performance-contract.json"
$apk = Join-Path $mobileRoot "androidApp\build\outputs\apk\full\release\androidApp-full-release.apk"

function Invoke-Checked {
    param([Parameter(Mandatory = $true)][scriptblock]$Command, [string]$Label)
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "$Label failed with exit code $LASTEXITCODE." }
}

Push-Location $mobileRoot
try {
    if (-not $SkipBuild) {
        Invoke-Checked -Label "Full release build" -Command {
            & .\gradlew.bat :androidApp:assembleFullRelease --no-parallel --max-workers=1 `
                --console=plain
        }
    }
    Invoke-Checked -Label "Android artifact verification" -Command {
        & py -3 scripts\verify_android_performance_artifact.py --apk $apk --contract $contract
    }
    if ($SkipBenchmark) { return }

    $adb = Get-Command adb -ErrorAction SilentlyContinue
    if (-not $adb) {
        $sdkRoot = @($env:ANDROID_SDK_ROOT, $env:ANDROID_HOME) |
            Where-Object { $_ } | Select-Object -First 1
        if ($sdkRoot) { $adb = Get-Item (Join-Path $sdkRoot "platform-tools\adb.exe") -ErrorAction SilentlyContinue }
    }
    if (-not $adb) { throw "adb is required for the Android performance benchmark." }
    $adbPath = if ($adb.Source) { $adb.Source } else { $adb.FullName }

    if ([string]::IsNullOrWhiteSpace($Serial)) {
        $devices = @(& $adbPath devices | Select-Object -Skip 1 | ForEach-Object {
            if ($_ -match '^([^\s]+)\s+device$') { $matches[1] }
        })
        if ($devices.Count -ne 1) {
            throw "Pass -Serial when adb does not expose exactly one ready device."
        }
        $Serial = $devices[0]
    }
    & $adbPath -s $Serial get-state | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Android device $Serial is not ready." }
    $isEmulator = ((& $adbPath -s $Serial shell getprop ro.kernel.qemu).Trim() -eq "1")
    if ($isEmulator -and -not $AllowEmulatorDiagnostics) {
        throw "Macrobenchmark budgets require a physical device; use -AllowEmulatorDiagnostics only for non-gating traces."
    }

    $started = Get-Date
    $savedSerial = $env:ANDROID_SERIAL
    try {
        $env:ANDROID_SERIAL = $Serial
        Invoke-Checked -Label "Android Macrobenchmark" -Command {
            & .\gradlew.bat :baselineprofile:connectedFullBenchmarkReleaseAndroidTest `
                --no-parallel --max-workers=1 --console=plain
        }
    }
    finally {
        if ($null -eq $savedSerial) { Remove-Item Env:ANDROID_SERIAL -ErrorAction SilentlyContinue }
        else { $env:ANDROID_SERIAL = $savedSerial }
    }

    $results = @(Get-ChildItem baselineprofile\build\outputs -Recurse `
        -Filter "*-benchmarkData.json" -File | Where-Object { $_.LastWriteTime -ge $started })
    if ($results.Count -eq 0) { throw "Macrobenchmark produced no fresh benchmarkData JSON." }
    $arguments = @(
        "-3", "scripts\verify_android_benchmark.py", "--results"
    ) + @($results.FullName) + @("--contract", $contract)
    if ($isEmulator) { $arguments += "--diagnostic-only" }
    & py @arguments
    if ($LASTEXITCODE -ne 0) { throw "Android benchmark contract failed." }
}
finally {
    Pop-Location
}
