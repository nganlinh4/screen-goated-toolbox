param(
    [ValidateSet("All", "Windows", "Android")]
    [string]$Platform = "All",
    [switch]$SkipAndroidBuild,
    [int]$Top = 40,
    [int]$BuildJobs = 2
)

$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot))
$cacheRoot = if ([string]::IsNullOrWhiteSpace($env:SGT_DEV_CACHE_ROOT)) {
    Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)) `
        "SGT-Development\cache"
}
else { [IO.Path]::GetFullPath($env:SGT_DEV_CACHE_ROOT) }
$ownershipRoot = Join-Path $cacheRoot "performance\ownership"
New-Item -ItemType Directory -Path $ownershipRoot -Force | Out-Null
. (Join-Path $PSScriptRoot "source-fingerprint.ps1")

if ($Top -lt 1 -or $Top -gt 500) { throw "Top must be between 1 and 500." }
if ($BuildJobs -lt 1 -or $BuildJobs -gt 16) { throw "BuildJobs must be between 1 and 16." }

function Invoke-WindowsOwnership {
    $cargoBloat = Get-Command cargo-bloat -ErrorAction SilentlyContinue
    if (-not $cargoBloat) { throw "cargo-bloat is required: cargo install cargo-bloat" }
    $llvmSize = Get-Command llvm-size -ErrorAction SilentlyContinue
    if (-not $llvmSize) { throw "llvm-size is required for PE section attribution." }

    $target = Join-Path $ownershipRoot "windows-target"
    New-Item -ItemType Directory -Path $target -Force | Out-Null
    $savedNonshipping = $env:SGT_NONSHIPPING_PERFORMANCE_BUILD
    try {
        $env:SGT_NONSHIPPING_PERFORMANCE_BUILD = "1"
        $raw = & cargo bloat --locked --jobs $BuildJobs --profile release-compact `
            --target x86_64-pc-windows-msvc --target-dir $target `
            --bin screen-goated-toolbox --crates --message-format json -n 0
        if ($LASTEXITCODE -ne 0) { throw "cargo-bloat failed with exit code $LASTEXITCODE." }
    }
    finally {
        if ($null -eq $savedNonshipping) {
            Remove-Item Env:SGT_NONSHIPPING_PERFORMANCE_BUILD -ErrorAction SilentlyContinue
        }
        else { $env:SGT_NONSHIPPING_PERFORMANCE_BUILD = $savedNonshipping }
    }
    $bloat = ($raw -join "`n") | ConvertFrom-Json
    $exe = Join-Path $target `
        "x86_64-pc-windows-msvc\release-compact\screen-goated-toolbox.exe"
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
        throw "Windows ownership build produced no executable: $exe"
    }
    $sectionRows = @()
    foreach ($line in & $llvmSize.Source -A $exe) {
        if ($line -match '^\s*(\S+)\s+(\d+)\s+\d+\s*$') {
            $sectionRows += [ordered]@{ name = $matches[1]; bytes = [long]$matches[2] }
        }
    }
    $file = Get-Item -LiteralPath $exe
    [ordered]@{
        artifact = [ordered]@{
            path = $file.FullName
            bytes = $file.Length
            sha256 = (Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        peSections = $sectionRows
        crates = $bloat
    }
}

function Find-ApkAnalyzer {
    $roots = @($env:ANDROID_SDK_ROOT, $env:ANDROID_HOME,
        (Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)) `
            "Android\Sdk"),
        (Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::UserProfile)) `
            "android-sdk")) | Where-Object { $_ -and (Test-Path -LiteralPath $_) } |
        Select-Object -Unique
    foreach ($root in $roots) {
        $candidates = @(Get-ChildItem (Join-Path $root "cmdline-tools") -Directory `
            -ErrorAction SilentlyContinue | Sort-Object Name -Descending | ForEach-Object {
                Join-Path $_.FullName "bin\apkanalyzer.bat"
            })
        foreach ($candidate in $candidates) {
            if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
        }
    }
    throw "apkanalyzer.bat was not found under ANDROID_SDK_ROOT or ANDROID_HOME."
}

function Get-ZipGroups {
    param([Parameter(Mandatory = $true)][string]$Path)
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($Path)
    try {
        @($archive.Entries | ForEach-Object {
            $group = ($_.FullName -split '/')[0]
            if ($_.FullName -match '^classes\d*\.dex$') { $group = "DEX" }
            elseif ($_.FullName -match '^assets/') { $group = "assets" }
            elseif ($_.FullName -match '^res/') { $group = "res" }
            elseif ($_.FullName -match '^lib/') { $group = "lib" }
            [pscustomobject]@{ group = $group; raw = $_.Length; compressed = $_.CompressedLength }
        } | Group-Object group | ForEach-Object {
            [ordered]@{
                group = $_.Name
                rawBytes = [long](($_.Group | Measure-Object raw -Sum).Sum)
                compressedBytes = [long](($_.Group | Measure-Object compressed -Sum).Sum)
            }
        } | Sort-Object compressedBytes -Descending)
    }
    finally { $archive.Dispose() }
}

function Invoke-AndroidOwnership {
    $mobileRoot = Join-Path $repoRoot "mobile"
    $apk = Join-Path $mobileRoot `
        "androidApp\build\outputs\apk\full\release\androidApp-full-release.apk"
    if (-not $SkipAndroidBuild) {
        Push-Location $mobileRoot
        try {
            & .\gradlew.bat :androidApp:assembleFullRelease --no-parallel --max-workers=1 `
                --console=plain
            if ($LASTEXITCODE -ne 0) { throw "Android release build failed." }
        }
        finally { Pop-Location }
    }
    if (-not (Test-Path -LiteralPath $apk -PathType Leaf)) { throw "Missing APK: $apk" }
    $mapping = Join-Path $mobileRoot "androidApp\build\outputs\mapping\fullRelease"
    if (-not (Test-Path -LiteralPath (Join-Path $mapping "mapping.txt") -PathType Leaf)) {
        throw "Missing Full release R8 mapping: $mapping"
    }
    $analyzer = Find-ApkAnalyzer
    $raw = & $analyzer dex packages --defined-only --proguard-folder $mapping $apk
    if ($LASTEXITCODE -ne 0) { throw "apkanalyzer dex packages failed." }
    $packages = @()
    foreach ($line in $raw) {
        if ($line -match '^P\s+\S+\s+(\d+)\s+(\d+)\s+(\d+)\s+(.+)$') {
            $name = $matches[4]
            $packages += [pscustomobject]@{
                name = $name
                references = [long]$matches[1]
                definitions = [long]$matches[2]
                bytes = [long]$matches[3]
                depth = if ($name -eq "<TOTAL>") { 0 } else { ($name.Split('.')).Count }
            }
        }
    }
    $file = Get-Item -LiteralPath $apk
    [ordered]@{
        artifact = [ordered]@{
            path = $file.FullName
            bytes = $file.Length
            sha256 = (Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        zipGroups = Get-ZipGroups -Path $file.FullName
        topLevelPackages = @($packages | Where-Object {
            $_.depth -eq 1 -and $_.name -ne "<TOTAL>"
        } | Sort-Object bytes -Descending | Select-Object -First $Top name, definitions, bytes)
        retainedPackages = @($packages | Where-Object {
            $_.depth -le 7 -and $_.name -ne "<TOTAL>"
        } | Sort-Object bytes -Descending | Select-Object -First $Top name, definitions, bytes)
        productPackages = @($packages | Where-Object {
            $_.name -like "dev.screengoated.toolbox.mobile*" -and $_.depth -ge 5
        } | Sort-Object bytes -Descending | Select-Object -First $Top name, definitions, bytes)
    }
}

Push-Location $repoRoot
try {
    $payload = [ordered]@{
        schemaVersion = 1
        generatedAt = (Get-Date).ToUniversalTime().ToString("o")
        sourceFingerprint = Get-SgtSourceFingerprint -RepoRoot $repoRoot
    }
    if ($Platform -in @("All", "Windows")) { $payload.windows = Invoke-WindowsOwnership }
    if ($Platform -in @("All", "Android")) { $payload.android = Invoke-AndroidOwnership }
    $report = Join-Path $ownershipRoot ("binary-ownership-{0}.json" -f `
        (Get-Date -Format "yyyyMMdd-HHmmss"))
    $payload | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $report -Encoding utf8
    Write-Host "Binary ownership report: $report"
    if ($payload.windows) {
        Write-Host "Windows: $($payload.windows.artifact.bytes) bytes"
    }
    if ($payload.android) {
        Write-Host "Android: $($payload.android.artifact.bytes) bytes"
        $payload.android.topLevelPackages | Format-Table name, definitions, bytes -AutoSize
    }
}
finally { Pop-Location }
