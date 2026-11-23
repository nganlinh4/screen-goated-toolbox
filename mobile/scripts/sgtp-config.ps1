# Device-helper settings for sgtp.ps1. They name one developer's phone and LAN
# address, so they live in the per-user development folder, never in the
# checkout. Older in-tree files are read once as a migration source.

function Get-SgtpConfigPath {
    if ($env:SGT_SGTP_CONFIG) {
        return [IO.Path]::GetFullPath($env:SGT_SGTP_CONFIG)
    }
    if (-not $env:LOCALAPPDATA) {
        throw "LOCALAPPDATA is not set; set SGT_SGTP_CONFIG to the sgtp settings file."
    }
    return (Join-Path $env:LOCALAPPDATA "SGT-Development\sgtp.json")
}

function Get-SgtpLegacyConfigPaths([string]$RepoRoot) {
    $mobile = Join-Path $RepoRoot "mobile"
    return @((Join-Path $mobile ".sgtp.json"), (Join-Path $mobile ".adb-phone.json"))
}

function Get-SgtpDefaultConfig([string]$RepoRoot) {
    return @{
        mode = "tcpip"
        host = $null
        port = 5555
        packageName = "dev.screengoated.toolbox.mobile.debug"
        apkPath = (Join-Path $RepoRoot "mobile\androidApp\build\outputs\apk\full\debug\androidApp-full-debug.apk")
    }
}

function Read-SgtpConfig([string]$RepoRoot) {
    $config = Get-SgtpDefaultConfig $RepoRoot
    $path = Get-SgtpConfigPath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        $path = Get-SgtpLegacyConfigPaths $RepoRoot |
            Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
            Select-Object -First 1
    }
    if ($path) {
        $stored = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
        foreach ($property in $stored.PSObject.Properties) {
            $config[$property.Name] = $property.Value
        }
    }
    return $config
}

function Write-SgtpConfig([hashtable]$Config) {
    $path = Get-SgtpConfigPath
    New-Item -ItemType Directory -Path (Split-Path -Parent $path) -Force | Out-Null
    ($Config | ConvertTo-Json) | Set-Content -LiteralPath $path -Encoding UTF8
}
