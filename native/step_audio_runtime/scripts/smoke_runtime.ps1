param(
    [string]$Text = "Hello from Step Audio.",
    [string]$Voice = "default_en",
    [string]$OutputPath = (Join-Path $env:TEMP "step-audio-smoke.wav"),
    [double]$MinSeconds = 0.45
)

$ErrorActionPreference = "Stop"

$Root = Join-Path $env:LOCALAPPDATA "screen-goated-toolbox"
$RuntimeExe = Join-Path $Root "bin\step_audio_runtime\step-audio-sidecar\step-audio-sidecar.exe"
$ModelDir = Join-Path $Root "models\step_audio_editx\editx_awq"
$TokenizerDir = Join-Path $Root "models\step_audio_editx\tokenizer"

if (!(Test-Path $RuntimeExe)) {
    throw "Step Audio runtime entrypoint not found: $RuntimeExe"
}
if (!(Test-Path $ModelDir)) {
    throw "Step Audio AWQ model directory not found: $ModelDir"
}
if (!(Test-Path $TokenizerDir)) {
    throw "Step Audio tokenizer directory not found: $TokenizerDir"
}
if (Test-Path $OutputPath) {
    Remove-Item $OutputPath -Force
}

$Request = [ordered]@{
    id = "step-audio-smoke"
    text = $Text
    voice = $Voice
    stepModelDir = $ModelDir
    tokenizerDir = $TokenizerDir
    outputWavPath = $OutputPath
    promptAudioPath = ""
    promptText = ""
} | ConvertTo-Json -Compress

$ResponseText = $Request | & $RuntimeExe
Write-Output $ResponseText
$Response = $ResponseText | ConvertFrom-Json
if (!$Response.ok) {
    throw "Step Audio smoke failed: $($Response.error)"
}
if (!(Test-Path $OutputPath)) {
    throw "Step Audio smoke reported success but did not create: $OutputPath"
}

$Output = Get-Item $OutputPath
$MinBytes = [int]([Math]::Ceiling($Response.sampleRate * $MinSeconds * 2) + 44)
if ($Output.Length -lt $MinBytes) {
    throw "Step Audio smoke created too little audio: $($Output.Length) bytes; expected at least $MinBytes bytes"
}

Write-Output ("WAV {0} {1} bytes" -f $Output.FullName, $Output.Length)
