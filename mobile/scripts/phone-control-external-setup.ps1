function Read-ShizukuInstallRouteTaskStamp {
    $recents = (
        Invoke-TargetAdb -AdbArguments @("shell", "dumpsys", "activity", "recents")
    ) -join "`n"
    $stamps = foreach ($match in [regex]::Matches(
        $recents,
        '(?ms)^\s*\* Recent #\d+:.*?(?=^\s*\* Recent #|\z)'
    )) {
        $task = $match.Value
        if ($task.Contains("moe.shizuku.privileged.api") -or
            $task.Contains("shizuku.rikka.app/download")) {
            $stamp = [regex]::Match($task, '(?m)^\s*lastActiveTime=(?<value>\d+)')
            if ($stamp.Success) { [long]$stamp.Groups["value"].Value }
        }
    }
    if (@($stamps).Count -eq 0) { return 0L }
    return [long](($stamps | Measure-Object -Maximum).Maximum)
}

function Assert-NewShizukuInstallRouteTask {
    param([Parameter(Mandatory = $true)][long]$PreviousStamp)

    for ($attempt = 1; $attempt -le 50; $attempt += 1) {
        $currentStamp = Read-ShizukuInstallRouteTaskStamp
        if ($currentStamp -gt $PreviousStamp) {
            Write-Host "Verified official Shizuku setup handoff on $serial"
            return
        }
        Start-Sleep -Milliseconds 200
    }
    throw "Phone Control did not dispatch a new official Shizuku setup route."
}
