param(
    [Parameter(Mandatory = $true)]
    [string]$CandidateDir,
    [string]$DeviceSerial,
    [string]$HostFixtureDir
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-Native {
    param([string]$FilePath, [string[]]$Arguments)
    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$FilePath failed with exit code $LASTEXITCODE"
    }
}

function Get-Sha256 {
    param([string]$Path)
    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Assert-Identity {
    param([string]$Path, [long]$ByteCount, [string]$Sha256)
    $item = Get-Item -LiteralPath $Path
    if ($item.Length -ne $ByteCount -or (Get-Sha256 $Path) -ne $Sha256) {
        throw "Fixture identity differs: $Path"
    }
}

function Get-VerifiedDownload {
    param([string]$Uri, [string]$Path, [long]$ByteCount, [string]$Sha256)
    if (Test-Path -LiteralPath $Path) {
        Assert-Identity $Path $ByteCount $Sha256
        return
    }
    $temporary = "$Path.part"
    if (Test-Path -LiteralPath $temporary) {
        throw "Preserving unexpected partial fixture: $temporary"
    }
    Invoke-WebRequest -Uri $Uri -OutFile $temporary
    try {
        Assert-Identity $temporary $ByteCount $Sha256
        [IO.File]::Move($temporary, $Path)
    } catch {
        if (Test-Path -LiteralPath $temporary) { [IO.File]::Delete($temporary) }
        throw
    }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$mobileRoot = Join-Path $repoRoot "mobile"
$specRoot = Join-Path $mobileRoot "native\ort-runtime"
$smokeInputs = Get-Content -Raw -LiteralPath (Join-Path $specRoot "smoke-inputs.json") |
    ConvertFrom-Json
$embedded = Get-Content -Raw -LiteralPath (Join-Path $specRoot "embedded-models.json") |
    ConvertFrom-Json

$candidateRoot = (Resolve-Path -LiteralPath $CandidateDir).Path
$proxy = Join-Path $candidateRoot "libonnxruntime.so"
$real = Join-Path $candidateRoot "libonnxruntime_real.so"
$smoke = Join-Path $candidateRoot "moonshine-ort-smoke"
foreach ($path in @($proxy, $real, $smoke)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Candidate output is missing: $path"
    }
}

if (-not $HostFixtureDir) {
    $HostFixtureDir = Join-Path ([IO.Path]::GetTempPath()) "sgt-ort-runtime-smoke-fixture"
}
$fixtureRoot = [IO.Path]::GetFullPath($HostFixtureDir)
New-Item -ItemType Directory -Force -Path $fixtureRoot | Out-Null
$modelRoot = Join-Path $fixtureRoot "model"
New-Item -ItemType Directory -Force -Path $modelRoot | Out-Null
foreach ($file in $smokeInputs.model.files) {
    Get-VerifiedDownload "$($smokeInputs.model.baseUrl)/$($file.name)" `
        (Join-Path $modelRoot $file.name) $file.byteCount $file.sha256
}
$audio = Join-Path $fixtureRoot "two_cities.wav"
Get-VerifiedDownload $smokeInputs.audio.url $audio `
    $smokeInputs.audio.byteCount $smokeInputs.audio.sha256

$moonshineArchive = Join-Path $mobileRoot `
    "androidApp\libs\$($embedded.sourceArchive.fileName)"
Assert-Identity $moonshineArchive $embedded.sourceArchive.byteCount `
    $embedded.sourceArchive.sha256
$moonshine = Join-Path $fixtureRoot $embedded.sourceArchive.entry.fileName
if (-not (Test-Path -LiteralPath $moonshine)) {
    $zip = [IO.Compression.ZipFile]::OpenRead($moonshineArchive)
    try {
        $entry = $zip.GetEntry($embedded.sourceArchive.entry.fileName)
        if (-not $entry) { throw "Moonshine runtime entry is missing" }
        $input = $entry.Open()
        try {
            $output = [IO.File]::Open($moonshine, [IO.FileMode]::CreateNew)
            try { $input.CopyTo($output) } finally { $output.Dispose() }
        } finally { $input.Dispose() }
    } finally { $zip.Dispose() }
}
Assert-Identity $moonshine $embedded.sourceArchive.entry.byteCount `
    $embedded.sourceArchive.entry.sha256

$devices = @(
    adb devices | Select-Object -Skip 1 |
        Where-Object { $_ -match "^([^\s]+)\s+device(?:\s|$)" } |
        ForEach-Object { [regex]::Match($_, "^([^\s]+)").Groups[1].Value }
)
if ($DeviceSerial) {
    if ($DeviceSerial -notin $devices) { throw "ADB device is unavailable: $DeviceSerial" }
} elseif ($devices.Count -eq 1) {
    $DeviceSerial = $devices[0]
} else {
    throw "Expected exactly one ADB device or pass -DeviceSerial; found $($devices.Count)"
}

$remoteRoot = "/data/local/tmp/sgt-ort-runtime-smoke"
$remoteModel = "$remoteRoot/model"
Invoke-Native adb @("-s", $DeviceSerial, "shell", "mkdir", "-p", $remoteModel)
foreach ($file in @($proxy, $real, $moonshine, $smoke, $audio)) {
    Invoke-Native adb @("-s", $DeviceSerial, "push", $file, "$remoteRoot/")
}
foreach ($file in $smokeInputs.model.files) {
    Invoke-Native adb @(
        "-s", $DeviceSerial, "push", (Join-Path $modelRoot $file.name), "$remoteModel/"
    )
}
Invoke-Native adb @("-s", $DeviceSerial, "shell", "chmod", "0755", "$remoteRoot/moonshine-ort-smoke")
$command = "cd $remoteRoot && LD_LIBRARY_PATH=$remoteRoot " +
    "./moonshine-ort-smoke $remoteModel $remoteRoot/two_cities.wav"
$output = & adb -s $DeviceSerial shell $command 2>&1
if ($LASTEXITCODE -ne 0) { throw "Physical-device transcription failed:`n$output" }
$transcript = $output -join "`n"
if ($transcript -notmatch [regex]::Escape($smokeInputs.expectedTranscript)) {
    throw "Expected transcript was not produced: $($smokeInputs.expectedTranscript)`n$transcript"
}
Write-Host $transcript
Write-Host "Physical-device ORT proxy transcription passed on $DeviceSerial"
