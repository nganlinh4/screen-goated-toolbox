param(
    [switch]$Package
)

$ErrorActionPreference = "Stop"

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$NativeDir = Join-Path $RepoRoot "mobile\androidApp\native\ort_proxy"
$BuildDir = Join-Path $RepoRoot "mobile\androidApp\build\ort_proxy"
$OrtZip = Join-Path $RepoRoot "mobile\androidApp\libs\ort-runtime.zip"
$OutputLib = Join-Path $BuildDir "libonnxruntime.so"

$AndroidSdk = if ($env:ANDROID_SDK_ROOT) { $env:ANDROID_SDK_ROOT } elseif ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { "C:\Users\user\android-sdk" }
$NdkVersion = if ($env:NDK_VERSION) { $env:NDK_VERSION } else { "27.0.12077973" }
$AndroidNdk = if ($env:ANDROID_NDK_ROOT) { $env:ANDROID_NDK_ROOT } else { Join-Path $AndroidSdk "ndk\$NdkVersion" }
$CmakeBin = Join-Path $AndroidSdk "cmake\3.22.1\bin\cmake.exe"
$NinjaBin = Join-Path $AndroidSdk "cmake\3.22.1\bin\ninja.exe"
$Toolchain = Join-Path $AndroidNdk "build\cmake\android.toolchain.cmake"
# Resolve from whatever gradle transform cache currently holds the ORT AAR
# (the cache hash changes across gradle versions).
$OrtRuntime = Get-ChildItem (Join-Path $env:USERPROFILE ".gradle\caches\*\transforms\*\transformed\jetified-onnxruntime-android-*\jni\arm64-v8a\libonnxruntime.so") -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1 -ExpandProperty FullName
$CxxRuntime = Join-Path $AndroidNdk "toolchains\llvm\prebuilt\windows-x86_64\sysroot\usr\lib\aarch64-linux-android\libc++_shared.so"

if (!(Test-Path $CmakeBin)) { throw "Missing cmake.exe at $CmakeBin" }
if (!(Test-Path $NinjaBin)) { throw "Missing ninja.exe at $NinjaBin" }
if (!(Test-Path $Toolchain)) { throw "Missing Android toolchain at $Toolchain" }
if (!(Test-Path $OrtRuntime)) { throw "Missing ORT runtime at $OrtRuntime" }
if (!(Test-Path $CxxRuntime)) { throw "Missing libc++ runtime at $CxxRuntime" }

if (Test-Path $BuildDir) {
    Remove-Item $BuildDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $BuildDir | Out-Null

& $CmakeBin -S $NativeDir -B $BuildDir `
    -G Ninja `
    "-DCMAKE_MAKE_PROGRAM=$NinjaBin" `
    -DCMAKE_BUILD_TYPE=Release `
    -DANDROID_ABI=arm64-v8a `
    -DANDROID_PLATFORM=android-29 `
    "-DCMAKE_TOOLCHAIN_FILE=$Toolchain"

& $CmakeBin --build $BuildDir

if (!(Test-Path $OutputLib)) {
    throw "Expected output library not found: $OutputLib"
}

Write-Host "Built $OutputLib"

if ($Package) {
    $TmpDir = Join-Path $env:TEMP ("ort-proxy-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $TmpDir | Out-Null
    try {
        Copy-Item $CxxRuntime (Join-Path $TmpDir "libc++_shared.so") -Force
        Copy-Item $OutputLib (Join-Path $TmpDir "libonnxruntime.so") -Force
        Copy-Item $OrtRuntime (Join-Path $TmpDir "libonnxruntime_real.so") -Force
        if (Test-Path $OrtZip) {
            Remove-Item $OrtZip -Force
        }
        Compress-Archive -Path (Join-Path $TmpDir "*") -DestinationPath $OrtZip
        Write-Host "Updated $OrtZip"
    } finally {
        if (Test-Path $TmpDir) {
            Remove-Item $TmpDir -Recurse -Force
        }
    }
}
