$ErrorActionPreference = "Stop"

$vswhere = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
$vsSetup = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\setup.exe"

if (!(Test-Path $vswhere)) {
    throw "vswhere.exe not found at $vswhere"
}

if (!(Test-Path $vsSetup)) {
    throw "Visual Studio setup.exe not found at $vsSetup"
}

$vs = & $vswhere -latest -products * -property installationPath
if (!$vs) {
    throw "Could not find a Visual Studio installation path."
}

Write-Host "Visual Studio install path: $vs"
Write-Host "Installing ARM64 MSVC tools..."

& $vsSetup modify `
  --installPath $vs `
  --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 `
  --passive `
  --norestart `
  --force

Write-Host "Installing Rust target aarch64-pc-windows-msvc..."
rustup target add aarch64-pc-windows-msvc

if (-not (Get-Command clang -ErrorAction SilentlyContinue)) {
    Write-Host "Installing LLVM/clang via winget..."
    winget install --id LLVM.LLVM --accept-source-agreements --accept-package-agreements
} else {
    Write-Host "clang already present on PATH."
}

$msvc = (Get-ChildItem "$vs\VC\Tools\MSVC" | Sort-Object Name -Descending | Select-Object -First 1).FullName

Write-Host ""
Write-Host "Verification:"
Write-Host "MSVC root: $msvc"
Write-Host ""
Write-Host "Hostx64 bins:"
Get-ChildItem "$msvc\bin\Hostx64" | Select-Object -ExpandProperty Name
Write-Host ""
Write-Host "MSVC libs:"
Get-ChildItem "$msvc\lib" | Select-Object -ExpandProperty Name
Write-Host ""
Write-Host "Installed Rust targets:"
rustup target list --installed
Write-Host ""
Write-Host "clang path:"
$clang = Get-Command clang -ErrorAction SilentlyContinue
if ($clang) {
    $clang.Source
} else {
    Write-Host "clang not found on PATH"
}
Write-Host ""
Write-Host "If you see 'arm64' in both lists above and clang on PATH, the toolchain is ready."
