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

function ConvertTo-DiagnosticValue {
    param([Parameter(Mandatory = $true)][string]$Value)

    if ($Value -eq "true") { return $true }
    if ($Value -eq "false") { return $false }
    [long]$integer = 0
    if ([long]::TryParse($Value, [ref]$integer)) { return $integer }
    return $Value
}

function ConvertTo-StructuralRecord {
    param([Parameter(Mandatory = $true)][psobject]$Record)

    $schema = if ($Record.PSObject.Properties["schema_version"]) {
        [int]$Record.schema_version
    } else {
        1
    }
    $fields = [ordered]@{}
    if ($schema -ge 2 -and $Record.PSObject.Properties["fields"] -and $Record.fields) {
        foreach ($property in $Record.fields.PSObject.Properties) {
            $fields[$property.Name] = $property.Value
        }
        $eventName = [string]$Record.event
    } else {
        $message = ([string]$Record.event -replace "\s+", " ").Trim()
        $tokens = @($message -split " " | Where-Object { $_ })
        $eventName = if ($tokens.Count -gt 0 -and $tokens[0] -match "^[A-Za-z][A-Za-z0-9_.-]*$") {
            $tokens[0]
        } else {
            "diagnostic_event"
        }
        foreach ($token in @($tokens | Select-Object -Skip 1)) {
            if ($token -match "^(?<key>[A-Za-z][A-Za-z0-9_.-]*)=(?<value>\S+)$") {
                $fields[$Matches.key] = ConvertTo-DiagnosticValue -Value $Matches.value
            }
        }
    }
    return [pscustomobject][ordered]@{
        schema_version = $schema
        session_id = if ($Record.PSObject.Properties["session_id"]) {
            [string]$Record.session_id
        } else {
            "legacy-$($Record.pid)"
        }
        sequence = if ($Record.PSObject.Properties["sequence"]) {
            [long]$Record.sequence
        } else {
            0L
        }
        timestamp_ms = [long]$Record.timestamp_ms
        elapsed_ms = [long]$Record.elapsed_ms
        level = [string]$Record.level
        tag = [string]$Record.tag
        event = $eventName
        fields = $fields
        throwable_type = if ($Record.PSObject.Properties["throwable_type"]) {
            [string]$Record.throwable_type
        } else {
            $null
        }
    }
}

function New-CountMap {
    param(
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][object[]]$Records,
        [Parameter(Mandatory = $true)][scriptblock]$Selector
    )

    $counts = [ordered]@{}
    @($Records | Group-Object -Property $Selector | Sort-Object Name) | ForEach-Object {
        $counts[[string]$_.Name] = $_.Count
    }
    return $counts
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

$records = [System.Collections.Generic.List[object]]::new()
$parseErrors = 0
$legacyInvalidationRecordsCompacted = 0L
$legacyHardInvalidationCount = 0L
$legacySemanticInvalidationCount = 0L
foreach ($journalName in @("events.previous.jsonl", "events.jsonl")) {
    $journalPath = Join-Path $resolvedOutput $journalName
    if (-not (Test-Path -LiteralPath $journalPath)) { continue }
    foreach ($line in [System.IO.File]::ReadLines($journalPath)) {
        if (-not $line.Trim()) { continue }
        if ($line.Contains('"event":"invalidation_summary')) {
            $legacyInvalidationRecordsCompacted += 1L
            if ($line -match "\bhard=(\d+)") {
                $legacyHardInvalidationCount += [long]$Matches[1]
            }
            if ($line -match "\bsemantic_only=(\d+)") {
                $legacySemanticInvalidationCount += [long]$Matches[1]
            }
            continue
        }
        try {
            $records.Add((ConvertTo-StructuralRecord -Record ($line | ConvertFrom-Json)))
        } catch {
            $parseErrors += 1
        }
    }
}
$orderedRecords = @($records | Sort-Object timestamp_ms, sequence)
$maxTimelineRecords = 600
$timelineRecords = @($orderedRecords | Select-Object -Last $maxTimelineRecords)
$timelineOmittedCount = [Math]::Max(0, $orderedRecords.Count - $timelineRecords.Count)
$timelineFields = @(
    "turn_id", "job_id", "name", "generation", "elapsed_ms", "code",
    "capability", "provider", "provider_state", "observation_generation",
    "attempted_observation_generation", "attempted_target_id",
    "attempted_visual_revision", "expected_visual_revision", "current_visual_revision",
    "target_snapshot_generation", "target_display_id", "target_window_id",
    "target_generation", "capture_generation", "current_generation",
    "certainty", "effect_status", "effect_verified", "snapshot_invalidated",
    "retryable", "fresh_observation_required", "fresh_observation_attached",
    "state_reconciled", "required_user_step", "hard", "semantic_since_hard",
    "event_type", "window_id", "visual_revision", "reason", "visited_nodes"
)
$timeline = foreach ($record in $timelineRecords) {
    $hardInvalidations = if ($record.fields.Contains("hard")) {
        [long]$record.fields["hard"]
    } else {
        0L
    }
    if ($record.event -eq "invalidation_summary" -and $hardInvalidations -eq 0) {
        continue
    }
    $when = [DateTimeOffset]::FromUnixTimeMilliseconds(
        $record.timestamp_ms
    ).ToUniversalTime().ToString("o")
    $pairs = foreach ($field in $timelineFields) {
        if ($record.fields.Contains($field)) {
            "$field=$($record.fields[$field])"
        }
    }
    $suffix = if (@($pairs).Count -gt 0) { " " + ($pairs -join " ") } else { "" }
    "$when $($record.level) $($record.event)$suffix"
}
@($timeline) | Set-Content -LiteralPath (
    Join-Path $resolvedOutput "timeline.txt"
) -Encoding utf8

$toolReceipts = @($orderedRecords | Where-Object { $_.event -eq "tool_receipt" })
$toolFailures = @(
    $toolReceipts | Where-Object {
        $_.fields.Contains("code") -and [string]$_.fields["code"] -ne "ok"
    }
)
$errorRecords = @(
    $orderedRecords | Where-Object {
        $_.level -eq "E" -or $_.event -match "(failed|error|exception)$"
    } | Select-Object -Last 100
)
$sequenceGapCount = 0L
@($orderedRecords | Where-Object { $_.schema_version -ge 2 } | Group-Object session_id) |
    ForEach-Object {
        $sequences = @($_.Group.sequence | Sort-Object -Unique)
        for ($index = 1; $index -lt $sequences.Count; $index += 1) {
            $gap = [long]$sequences[$index] - [long]$sequences[$index - 1] - 1L
            if ($gap -gt 0) { $sequenceGapCount += $gap }
        }
    }
$summary = [ordered]@{
    schema_version = 1
    record_count = $orderedRecords.Count
    timeline_count = @($timeline).Count
    timeline_omitted_count = $timelineOmittedCount
    parse_error_count = $parseErrors
    sequence_gap_count = $sequenceGapCount
    legacy_invalidation_records_compacted = $legacyInvalidationRecordsCompacted
    legacy_hard_invalidation_count = $legacyHardInvalidationCount
    legacy_semantic_invalidation_count = $legacySemanticInvalidationCount
    sessions = New-CountMap -Records $orderedRecords -Selector { $_.session_id }
    record_schemas = New-CountMap -Records $orderedRecords -Selector { $_.schema_version }
    events = New-CountMap -Records $orderedRecords -Selector { $_.event }
    tool_receipt_codes = New-CountMap -Records $toolReceipts -Selector {
        if ($_.fields.Contains("code")) { $_.fields["code"] } else { "unknown" }
    }
    tool_failure_count = $toolFailures.Count
    errors = @($errorRecords | ForEach-Object {
        [ordered]@{
            timestamp_ms = $_.timestamp_ms
            event = $_.event
            code = if ($_.fields.Contains("code")) { $_.fields["code"] } else { $null }
            throwable_type = $_.throwable_type
        }
    })
}
$summary | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (
    Join-Path $resolvedOutput "summary.json"
) -Encoding utf8

Write-Host "Phone Control diagnostics collected at $resolvedOutput"
