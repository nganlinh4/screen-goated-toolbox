param(
    [string]$PythonArchive = "..\..\youtubedl-android\library\src\main\jniLibs\arm64-v8a\libpython.zip.so",
    [string]$FfmpegArchive = "..\..\youtubedl-android\ffmpeg\src\main\jniLibs\arm64-v8a\libffmpeg.zip.so",
    [string]$OutputDirectory = "..\local-runtime-bundles\sgt_downloader_runtime"
)

$ErrorActionPreference = "Stop"
$mobileRoot = Split-Path -Parent $PSScriptRoot
$repoRoot = Split-Path -Parent $mobileRoot
$manifestPath = Join-Path $mobileRoot "androidApp\delivery\downloader-runtime.json"
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json

function Resolve-InputPath([string]$Path) {
    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $mobileRoot $Path))
}

function Copy-VerifiedArtifact([string]$Role, [string]$SourcePath, [string]$DestinationRoot) {
    $contract = $manifest.artifacts | Where-Object { $_.role -eq $Role }
    if ($null -eq $contract) { throw "Manifest is missing $Role" }
    $source = Get-Item -LiteralPath (Resolve-InputPath $SourcePath)
    if ($source.Length -ne [long]$contract.sizeBytes) {
        throw "$Role size mismatch: $($source.Length)"
    }
    $hash = (Get-FileHash -LiteralPath $source.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $contract.sha256) { throw "$Role SHA-256 mismatch" }
    $destination = Join-Path $DestinationRoot $contract.asset
    Copy-Item -LiteralPath $source.FullName -Destination $destination -Force
    Write-Output $destination
}

$resolvedOutput = if ([System.IO.Path]::IsPathRooted($OutputDirectory)) {
    [System.IO.Path]::GetFullPath($OutputDirectory)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $mobileRoot $OutputDirectory))
}
New-Item -ItemType Directory -Force -Path $resolvedOutput | Out-Null
Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $resolvedOutput "delivery.json") -Force
Copy-VerifiedArtifact "python" $PythonArchive $resolvedOutput
Copy-VerifiedArtifact "ffmpeg" $FfmpegArchive $resolvedOutput

Write-Output "Prepared immutable downloader artifacts in $resolvedOutput"
Write-Output "Upload only the two uniquely named ZIP files to the append-only sgt-runtime-bundles release."
