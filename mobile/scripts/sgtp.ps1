param(
    [ValidateSet("status", "connect", "pair", "enable-fixed-port", "install", "launch", "run", "logcat", "logcat-all", "disconnect", "help")]
    [string]$Action = "run",
    [string]$DeviceHost,
    [int]$Port,
    [string]$Package,
    [string]$Apk,
    [string]$PairEndpoint,
    [string]$PairCode,
    [string]$ConnectEndpoint
)

$ErrorActionPreference = "Stop"

function Get-RepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
}

function Get-DefaultConfig {
    $repoRoot = Get-RepoRoot
    return @{
        mode = "tcpip"
        host = "192.168.219.122"
        port = 5555
        packageName = "dev.screengoated.toolbox.mobile.debug"
        apkPath = (Join-Path $repoRoot "mobile\androidApp\build\outputs\apk\full\debug\androidApp-full-debug.apk")
    }
}

function Get-ConfigPath {
    return (Join-Path (Join-Path (Get-RepoRoot) "mobile") ".sgtp.json")
}

function Get-LegacyConfigPath {
    return (Join-Path (Join-Path (Get-RepoRoot) "mobile") ".adb-phone.json")
}

function Load-Config {
    $path = Get-ConfigPath
    $legacyPath = Get-LegacyConfigPath
    $config = Get-DefaultConfig
    if (-not (Test-Path $path) -and (Test-Path $legacyPath)) {
        $path = $legacyPath
    }
    if (Test-Path $path) {
        $stored = Get-Content $path -Raw | ConvertFrom-Json
        foreach ($property in $stored.PSObject.Properties) {
            $config[$property.Name] = $property.Value
        }
    }
    return $config
}

function Save-Config($config) {
    $path = Get-ConfigPath
    ($config | ConvertTo-Json) | Set-Content -Path $path -Encoding UTF8
    $legacyPath = Get-LegacyConfigPath
    if ((Test-Path $legacyPath) -and ($legacyPath -ne $path)) {
        Remove-Item $legacyPath -Force
    }
}

function Invoke-AdbCapture([string[]]$Arguments) {
    $output = & adb @Arguments 2>&1
    return [string]::Join([Environment]::NewLine, $output)
}

function Get-Endpoint($config) {
    return "$($config.host):$($config.port)"
}

function Split-Endpoint([string]$endpoint) {
    $hostPart, $portPart = $endpoint.Split(":")
    return @{
        host = $hostPart
        port = [int]$portPart
    }
}

function Same-Subnet([string]$leftHost, [string]$rightHost) {
    if (-not $leftHost -or -not $rightHost) {
        return $false
    }
    $leftParts = $leftHost.Split(".")
    $rightParts = $rightHost.Split(".")
    if ($leftParts.Length -ne 4 -or $rightParts.Length -ne 4) {
        return $false
    }
    return (($leftParts[0..2] -join ".") -eq ($rightParts[0..2] -join "."))
}

function Test-AdbDevice($serial) {
    try {
        $state = (& adb -s $serial get-state 2>$null).Trim()
        return $state -eq "device"
    } catch {
        return $false
    }
}

function Wait-ForAdbDevice([string]$serial, [int]$TimeoutSeconds = 5) {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        if (Test-AdbDevice $serial) {
            return $true
        }
        Start-Sleep -Milliseconds 300
    } while ((Get-Date) -lt $deadline)

    return $false
}

function Connect-Endpoint($endpoint) {
    $output = Invoke-AdbCapture @("connect", $endpoint)
    if (-not (Wait-ForAdbDevice $endpoint)) {
        throw "ADB connect failed for $endpoint`n$output"
    }
    return $output
}

function Find-EndpointOnSubnet($config) {
    if (-not $config.host) {
        return $null
    }

    $parts = $config.host.Split(".")
    if ($parts.Length -ne 4) {
        return $null
    }

    $prefix = "$($parts[0]).$($parts[1]).$($parts[2])"
    foreach ($suffix in 2..254) {
        $candidate = "$prefix.$suffix"
        if ($candidate -eq $config.host) {
            continue
        }

        $client = New-Object System.Net.Sockets.TcpClient
        try {
            $async = $client.BeginConnect($candidate, [int]$config.port, $null, $null)
            if (-not $async.AsyncWaitHandle.WaitOne(120)) {
                continue
            }
            $client.EndConnect($async)
            return $candidate
        } catch {
            continue
        } finally {
            $client.Dispose()
        }
    }

    return $null
}

function Get-MdnsServices {
    $services = @()
    $output = & adb mdns services 2>$null
    foreach ($line in $output) {
        $text = "$line".Trim()
        if (-not $text -or $text.StartsWith("List of discovered mdns services")) {
            continue
        }
        if ($text -match "(?<name>\S+)\s+_(?<kind>adb-tls-connect|adb-tls-pairing)\._tcp\.?\s+(?<host>\d+\.\d+\.\d+\.\d+):(?<port>\d+)") {
            $services += [pscustomobject]@{
                name = $Matches.name
                kind = if ($Matches.kind -eq "adb-tls-connect") { "connect" } else { "pairing" }
                host = $Matches.host
                port = [int]$Matches.port
                endpoint = "$($Matches.host):$($Matches.port)"
            }
        }
    }
    return $services
}

function Find-MdnsEndpoint([string]$kind, [hashtable]$config) {
    $services = @(Get-MdnsServices | Where-Object { $_.kind -eq $kind })
    if (-not $services) {
        return $null
    }

    $ranked = $services | Sort-Object `
        @{ Expression = { if ($config.host -and $_.host -eq $config.host) { 0 } elseif (Same-Subnet $_.host $config.host) { 1 } else { 2 } } }, `
        @{ Expression = { $_.host } }, `
        @{ Expression = { $_.port } }

    return $ranked[0].endpoint
}

function Wait-ForMdnsEndpoint([string]$kind, [hashtable]$config, [int]$TimeoutSeconds = 15) {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        $endpoint = Find-MdnsEndpoint $kind $config
        if ($endpoint) {
            return $endpoint
        }
        Start-Sleep -Milliseconds 800
    } while ((Get-Date) -lt $deadline)

    return $null
}

function Prompt-ForConnectEndpoint([hashtable]$config) {
    $manualEndpoint = Read-Host "Wireless debugging endpoint (use the 'IP address & Port' shown on the phone, example 192.168.1.50:41337)"
    if (-not $manualEndpoint) {
        throw "No wireless debugging endpoint provided."
    }
    $parts = Split-Endpoint $manualEndpoint
    $config.mode = "wireless-debugging"
    $config.host = $parts.host
    $config.port = $parts.port
    Connect-Endpoint $manualEndpoint | Out-Host
    Save-Config $config
    return $manualEndpoint
}

function Show-Usage {
    Write-Host "sgtp"
    Write-Host "  Normal use: connect + open filtered logs + install + launch"
    Write-Host ""
    Write-Host "Fallback only:"
    Write-Host "  sgtp pair   one-time Android wireless debugging pair if trust is lost"
    Write-Host "  sgtp status show saved endpoint and current adb devices"
}

function Ensure-Connected([hashtable]$config) {
    if ($DeviceHost) {
        $config.host = $DeviceHost
    }
    if ($Port) {
        $config.port = $Port
    }
    if ($Package) {
        $config.packageName = $Package
    }
    if ($Apk) {
        $config.apkPath = $Apk
    }

    $endpoint = Get-Endpoint $config
    if (Test-AdbDevice $endpoint) {
        Save-Config $config
        return $endpoint
    }

    $mdnsEndpoint = Find-MdnsEndpoint "connect" $config
    if ($mdnsEndpoint) {
        $parts = Split-Endpoint $mdnsEndpoint
        $config.host = $parts.host
        $config.port = $parts.port
        Connect-Endpoint $mdnsEndpoint | Out-Host
        Save-Config $config
        return $mdnsEndpoint
    }

    try {
        Connect-Endpoint $endpoint | Out-Host
        Save-Config $config
        return $endpoint
    } catch {
        if ($config.mode -ne "tcpip") {
            throw
        }
    }

    $discoveredHost = Find-EndpointOnSubnet $config
    if ($discoveredHost) {
        $config.host = $discoveredHost
        $endpoint = Get-Endpoint $config
        Connect-Endpoint $endpoint | Out-Host
        Save-Config $config
        return $endpoint
    }

    return Prompt-ForConnectEndpoint $config
}

function Resolve-CurrentSerial($config) {
    $devices = & adb devices
    foreach ($line in $devices) {
        if ($line -match "^(?<serial>\S+)\s+device$") {
            if ($Matches.serial -eq "List") {
                continue
            }
            if ($config.host -and $Matches.serial.StartsWith("$($config.host):")) {
                return $Matches.serial
            }
            return $Matches.serial
        }
    }
    throw "No connected ADB device found."
}

function Start-FilteredLogcatWindow([string]$endpoint) {
    $command = 'adb -s {0} logcat | findstr /i "AndroidRuntime chromium cr_WebView WebView SGT LiveTranslate dev.screengoated toolbox"' -f $endpoint
    Start-Process -FilePath "cmd.exe" -ArgumentList "/k", $command | Out-Null
}

function Install-Apk([hashtable]$config, [string]$endpoint) {
    if (-not (Test-Path $config.apkPath)) {
        throw "APK not found at $($config.apkPath). Build it first."
    }
    & adb -s $endpoint install -r $config.apkPath
}

function Launch-App([hashtable]$config, [string]$endpoint) {
    & adb -s $endpoint shell monkey -p $config.packageName 1
}

$config = Load-Config

switch ($Action) {
    "help" {
        Show-Usage
    }

    "status" {
        $endpoint = Get-Endpoint $config
        Write-Host "Mode: $($config.mode)"
        Write-Host "Endpoint: $endpoint"
        Write-Host "Package: $($config.packageName)"
        Write-Host "APK: $($config.apkPath)"
        Write-Host ""
        & adb devices -l
    }

    "connect" {
        $endpoint = Ensure-Connected $config
        Write-Host "Ready: $endpoint"
    }

    "disconnect" {
        $endpoint = Get-Endpoint $config
        & adb disconnect $endpoint
    }

    "pair" {
        if (-not $PairEndpoint) {
            Write-Host "Waiting for Android pairing service. Open 'Pair device with pairing code' on the phone..."
            $PairEndpoint = Wait-ForMdnsEndpoint "pairing" $config
            if ($PairEndpoint) {
                Write-Host "Found pairing endpoint: $PairEndpoint"
            } else {
                $PairEndpoint = Read-Host "Pair endpoint (example 192.168.1.50:37123)"
            }
        }
        if (-not $PairCode) {
            $PairCode = Read-Host "Pair code"
        }

        & adb pair $PairEndpoint $PairCode | Out-Host
        if (-not $ConnectEndpoint) {
            Write-Host "Waiting for Android connect service..."
            $ConnectEndpoint = Wait-ForMdnsEndpoint "connect" $config
            if ($ConnectEndpoint) {
                Write-Host "Found connect endpoint: $ConnectEndpoint"
            } else {
                $ConnectEndpoint = Read-Host "Connect endpoint (example 192.168.1.50:41337)"
            }
        }
        Connect-Endpoint $ConnectEndpoint | Out-Host

        $config.mode = "wireless-debugging"
        $parts = Split-Endpoint $ConnectEndpoint
        $config.host = $parts.host
        $config.port = $parts.port
        Save-Config $config
        Write-Host "Saved wireless debugging endpoint: $ConnectEndpoint"
    }

    "enable-fixed-port" {
        $serial = Resolve-CurrentSerial $config
        if (-not $DeviceHost) {
            if ($serial -match "^(?<serialHost>\d+\.\d+\.\d+\.\d+):\d+$") {
                $config.host = $Matches.serialHost
            }
        } else {
            $config.host = $DeviceHost
        }
        if ($Port) {
            $config.port = $Port
        } else {
            $config.port = 5555
        }

        & adb -s $serial tcpip $config.port | Out-Host
        Start-Sleep -Seconds 2
        $config.mode = "tcpip"
        $endpoint = Ensure-Connected $config
        Write-Host "Fixed-port ADB ready: $endpoint"
    }

    "install" {
        $endpoint = Ensure-Connected $config
        Install-Apk $config $endpoint
    }

    "launch" {
        $endpoint = Ensure-Connected $config
        Launch-App $config $endpoint
    }

    "run" {
        $endpoint = Ensure-Connected $config
        Start-FilteredLogcatWindow $endpoint
        Install-Apk $config $endpoint
        Launch-App $config $endpoint
    }

    "logcat" {
        $endpoint = Ensure-Connected $config
        & adb -s $endpoint logcat -s "SGT-EdgeTTS:*" "SGT-TTS:*" "SGTGeminiLive:*" "SGTAudioCapture:*" "SGTOverlayPerf:*" "AndroidRuntime:*" "SGT:*" "LiveTranslate:*"
    }

    "logcat-all" {
        $endpoint = Ensure-Connected $config
        & adb -s $endpoint logcat
    }
}
