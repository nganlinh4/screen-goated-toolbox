param(
    [string]$LogPath = (Join-Path $env:LOCALAPPDATA 'SGT\logs\session.log'),
    [int]$Last = 20,
    [double]$MaxAppAfterProviderMs = 0,
    [switch]$Json
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $LogPath -PathType Leaf)) {
    throw "Overlay latency log not found: $LogPath"
}

$pattern = '\[OverlayPerf\] trace=(?<trace>\S+) phase=(?<phase>\S+) elapsed_ms=(?<ms>[0-9.]+)'
$traces = [ordered]@{}

foreach ($line in Get-Content -LiteralPath $LogPath) {
    $match = [regex]::Match($line, $pattern)
    if (-not $match.Success) {
        continue
    }
    $traceId = $match.Groups['trace'].Value
    if (-not $traces.Contains($traceId)) {
        $traces[$traceId] = [ordered]@{}
    }
    $traces[$traceId][$match.Groups['phase'].Value] =
        [double]::Parse($match.Groups['ms'].Value, [Globalization.CultureInfo]::InvariantCulture)
}

$samples = foreach ($entry in $traces.GetEnumerator()) {
    $phase = $entry.Value
    if (-not $phase.Contains('first_painted')) {
        continue
    }
    $providerFirst = if ($phase.Contains('provider_first_output')) {
        $phase['provider_first_output']
    } else {
        $null
    }
    [pscustomobject]@{
        Trace = $entry.Key
        ProviderFirstMs = $providerFirst
        FirstPaintMs = $phase['first_painted']
        AppAfterProviderMs = if ($null -ne $providerFirst) {
            [math]::Round($phase['first_painted'] - $providerFirst, 1)
        } else {
            $null
        }
        ProviderCompleteMs = $phase['provider_complete']
        FinalPaintMs = $phase['final_painted']
        FinalFitMs = $phase['final_fit_completed']
    }
}

$samples = @($samples | Select-Object -Last $Last)
if ($samples.Count -eq 0) {
    throw "No completed OverlayPerf traces were found in $LogPath"
}

function Get-Percentile([double[]]$Values, [double]$Percentile) {
    $sorted = @($Values | Sort-Object)
    if ($sorted.Count -eq 0) {
        return $null
    }
    $index = [math]::Max(0, [math]::Ceiling($Percentile * $sorted.Count) - 1)
    return [math]::Round($sorted[$index], 1)
}

$firstPaintValues = @($samples | ForEach-Object { $_.FirstPaintMs })
$appValues = @($samples | Where-Object { $null -ne $_.AppAfterProviderMs } |
    ForEach-Object { $_.AppAfterProviderMs })
$summary = [pscustomobject]@{
    Samples = $samples.Count
    FirstPaintP50Ms = Get-Percentile $firstPaintValues 0.50
    FirstPaintP95Ms = Get-Percentile $firstPaintValues 0.95
    AppAfterProviderP50Ms = Get-Percentile $appValues 0.50
    AppAfterProviderP95Ms = Get-Percentile $appValues 0.95
}

if ($Json) {
    [pscustomobject]@{ Summary = $summary; Traces = $samples } |
        ConvertTo-Json -Depth 4
} else {
    $samples | Format-Table -AutoSize
    $summary | Format-List
}

if ($MaxAppAfterProviderMs -gt 0) {
    $violations = @($samples | Where-Object {
        $null -ne $_.AppAfterProviderMs -and $_.AppAfterProviderMs -gt $MaxAppAfterProviderMs
    })
    if ($violations.Count -gt 0) {
        Write-Error "$($violations.Count) trace(s) exceeded the app latency budget of $MaxAppAfterProviderMs ms"
        exit 1
    }
}
