param(
    [Parameter(Mandatory = $true)][string]$Report,
    [string]$Contract = (Join-Path $PSScriptRoot "performance-contract.json"),
    [int]$ExpectedRuns = 3
)

$ErrorActionPreference = "Stop"
$Report = [IO.Path]::GetFullPath($Report)
$Contract = [IO.Path]::GetFullPath($Contract)
if ($ExpectedRuns -lt 1 -or $ExpectedRuns -gt 20) {
    throw "ExpectedRuns must be between 1 and 20."
}
$payload = Get-Content -LiteralPath $Report -Raw | ConvertFrom-Json
$policy = Get-Content -LiteralPath $Contract -Raw | ConvertFrom-Json
$artifacts = @($payload.artifacts)
$measurements = @($payload.measurements)

function Get-MedianValue {
    param([object[]]$Rows, [string]$Property)
    $values = @($Rows | ForEach-Object { $_.$Property } | Where-Object { $null -ne $_ } |
        Sort-Object)
    if ($values.Count -eq 0) { return $null }
    $values[[Math]::Floor($values.Count / 2)]
}

function Get-BudgetFailures {
    param([string]$Smoke, [object[]]$Samples, [object]$Budget)
    $failures = [Collections.Generic.List[string]]::new()
    $medianElapsed = Get-MedianValue -Rows $Samples -Property "elapsedMs"
    if ($medianElapsed -gt $Budget.maxMedianElapsedMs) {
        $failures.Add("$Smoke median ${medianElapsed}ms exceeds $($Budget.maxMedianElapsedMs)ms.")
    }
    $peakWorkingSet = ($Samples | Measure-Object peakWorkingSetBytes -Maximum).Maximum
    if ($peakWorkingSet -gt $Budget.maxPeakWorkingSetBytes) {
        $failures.Add("$Smoke peak working set $peakWorkingSet exceeds $($Budget.maxPeakWorkingSetBytes).")
    }
    $peakThreads = ($Samples | Measure-Object peakThreads -Maximum).Maximum
    if ($peakThreads -gt $Budget.maxPeakThreads) {
        $failures.Add("$Smoke peak thread count $peakThreads exceeds $($Budget.maxPeakThreads).")
    }
    $peakProcesses = ($Samples | Measure-Object peakProcessCount -Maximum).Maximum
    if ($peakProcesses -gt $Budget.maxPeakProcessCount) {
        $failures.Add("$Smoke peak process count $peakProcesses exceeds $($Budget.maxPeakProcessCount).")
    }
    if ($null -ne $Budget.maxMedianFirstPaintMs) {
        $firstPaint = Get-MedianValue -Rows $Samples -Property "firstPaintMedianMs"
        if ($null -eq $firstPaint) {
            $failures.Add("$Smoke produced no first-paint measurement.")
        }
        elseif ($firstPaint -gt $Budget.maxMedianFirstPaintMs) {
            $failures.Add("$Smoke first-paint median ${firstPaint}ms exceeds $($Budget.maxMedianFirstPaintMs)ms.")
        }
    }
    if ($null -ne $Budget.maxMedianFinalFitMs) {
        $finalFit = Get-MedianValue -Rows $Samples -Property "finalFitMedianMs"
        if ($null -eq $finalFit) {
            $failures.Add("$Smoke produced no final-fit measurement.")
        }
        elseif ($finalFit -gt $Budget.maxMedianFinalFitMs) {
            $failures.Add("$Smoke final-fit median ${finalFit}ms exceeds $($Budget.maxMedianFinalFitMs)ms.")
        }
    }
    $failures.ToArray()
}

function Get-OneArtifact {
    param([string]$Build)
    $matches = @($artifacts | Where-Object build -eq $Build)
    if ($matches.Count -ne 1) { throw "Expected one $Build artifact in $Report." }
    $matches[0]
}

$compact = Get-OneArtifact -Build "compact"
$candidatePolicy = [ordered]@{
    balanced = [ordered]@{
        artifact = Get-OneArtifact -Build "balanced"
        maxSizeRatio = [double]$policy.windows.maxBalancedSizeRatio
        maxLatencyRatio = [double]$policy.windows.maxBalancedLatencyRatio
        reasons = [Collections.Generic.List[string]]::new()
    }
    perf = [ordered]@{
        artifact = Get-OneArtifact -Build "perf"
        maxSizeRatio = [double]$policy.windows.maxPerfSizeRatio
        maxLatencyRatio = [double]$policy.windows.maxPerfLatencyRatio
        reasons = [Collections.Generic.List[string]]::new()
    }
}
$shippingFailures = [Collections.Generic.List[string]]::new()
if ($compact.bytes -gt $policy.windows.maxShippingBinaryBytes) {
    $shippingFailures.Add(
        "Compact binary is $($compact.bytes) bytes; contract allows $($policy.windows.maxShippingBinaryBytes)."
    )
}
foreach ($entry in $candidatePolicy.GetEnumerator()) {
    $ratio = [double]$entry.Value.artifact.bytes / [double]$compact.bytes
    $entry.Value["sizeRatio"] = $ratio
    if ($ratio -gt $entry.Value.maxSizeRatio) {
        $entry.Value.reasons.Add(
            "$($entry.Key)/compact size ratio $([Math]::Round($ratio, 3)) exceeds $($entry.Value.maxSizeRatio)."
        )
    }
}

foreach ($smoke in $policy.windows.requiredSmokes) {
    $samples = @{}
    foreach ($build in @("compact", "balanced", "perf")) {
        $samples[$build] = @($measurements | Where-Object {
            $_.build -eq $build -and $_.smoke -eq $smoke
        } | Sort-Object elapsedMs)
        if ($samples[$build].Count -ne $ExpectedRuns) {
            $message = "$build $smoke has $($samples[$build].Count) samples; expected $ExpectedRuns."
            if ($build -eq "compact") { $shippingFailures.Add($message) }
            else { $candidatePolicy[$build].reasons.Add($message) }
        }
    }
    $budget = $policy.windows.absoluteBudgets.$smoke
    if ($null -eq $budget) {
        $shippingFailures.Add("Performance contract has no absolute budget for $smoke.")
        continue
    }
    if ($samples.compact.Count -eq $ExpectedRuns) {
        foreach ($failure in Get-BudgetFailures -Smoke $smoke -Samples $samples.compact -Budget $budget) {
            $shippingFailures.Add($failure)
        }
    }
    $compactMedian = Get-MedianValue -Rows $samples.compact -Property "elapsedMs"
    foreach ($build in @("balanced", "perf")) {
        if ($samples[$build].Count -ne $ExpectedRuns) { continue }
        foreach ($failure in Get-BudgetFailures -Smoke $smoke -Samples $samples[$build] -Budget $budget) {
            $candidatePolicy[$build].reasons.Add($failure)
        }
        $candidateMedian = Get-MedianValue -Rows $samples[$build] -Property "elapsedMs"
        $ratio = [double]$candidateMedian / [double]$compactMedian
        if ($ratio -gt $candidatePolicy[$build].maxLatencyRatio) {
            $candidatePolicy[$build].reasons.Add(
                "$smoke median ratio $([Math]::Round($ratio, 3)) exceeds $($candidatePolicy[$build].maxLatencyRatio)."
            )
        }
    }
}

$candidates = [ordered]@{}
foreach ($entry in $candidatePolicy.GetEnumerator()) {
    $candidates[$entry.Key] = [ordered]@{
        eligible = $entry.Value.reasons.Count -eq 0
        sizeRatio = $entry.Value.sizeRatio
        reasons = @($entry.Value.reasons)
    }
}
$validation = [ordered]@{
    shipping = [ordered]@{
        eligible = $shippingFailures.Count -eq 0
        failures = @($shippingFailures)
    }
    candidates = $candidates
}
$payload.schemaVersion = 2
$payload | Add-Member -NotePropertyName validation -NotePropertyValue $validation -Force
$payload | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $Report -Encoding utf8

Write-Host "Windows shipping profile eligible: $($validation.shipping.eligible)"
foreach ($entry in $candidates.GetEnumerator()) {
    Write-Host "$($entry.Key) candidate eligible: $($entry.Value.eligible)"
    $entry.Value.reasons | ForEach-Object { Write-Host "  - $_" }
}
if ($shippingFailures.Count -gt 0) {
    throw "Windows shipping performance contract failed: $($shippingFailures -join ' ')"
}
