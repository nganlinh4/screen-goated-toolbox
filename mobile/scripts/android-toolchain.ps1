# Resolves the JDK and Android SDK from this machine instead of tracked paths.

function Resolve-ReleaseJavaHome {
    $javaHome = $env:JAVA_HOME
    if ([string]::IsNullOrWhiteSpace($javaHome)) {
        throw "JAVA_HOME is not set. Point it at an installed JDK (17 or newer); Gradle provisions its pinned daemon JDK itself."
    }
    $java = Join-Path $javaHome "bin\java.exe"
    if (-not (Test-Path -LiteralPath $java -PathType Leaf)) {
        throw "JAVA_HOME '$javaHome' has no bin\java.exe. Point it at an installed JDK."
    }
    return [IO.Path]::GetFullPath($javaHome)
}

function Read-LocalPropertiesSdkDir([string]$MobileDir) {
    $properties = Join-Path $MobileDir "local.properties"
    if (-not (Test-Path -LiteralPath $properties -PathType Leaf)) {
        return $null
    }
    foreach ($line in Get-Content -LiteralPath $properties) {
        if ($line -match '^\s*sdk\.dir\s*[=:]\s*(?<value>.+?)\s*$') {
            # Java properties escape ':' and '\' with a backslash.
            return [regex]::Replace($Matches.value, '\\(.)', '$1')
        }
    }
    return $null
}

function Resolve-AndroidSdkRoot([string]$MobileDir) {
    $candidates = @(
        @{ Source = "ANDROID_HOME"; Path = $env:ANDROID_HOME },
        @{ Source = "ANDROID_SDK_ROOT"; Path = $env:ANDROID_SDK_ROOT },
        @{ Source = "mobile\local.properties sdk.dir"; Path = (Read-LocalPropertiesSdkDir $MobileDir) }
    )
    foreach ($candidate in $candidates) {
        if ([string]::IsNullOrWhiteSpace($candidate.Path)) {
            continue
        }
        if (-not (Test-Path -LiteralPath $candidate.Path -PathType Container)) {
            throw "$($candidate.Source) points at '$($candidate.Path)', which is not a directory."
        }
        return [IO.Path]::GetFullPath($candidate.Path)
    }
    throw "Android SDK not found. Set ANDROID_HOME or ANDROID_SDK_ROOT, or sdk.dir in mobile\local.properties."
}
