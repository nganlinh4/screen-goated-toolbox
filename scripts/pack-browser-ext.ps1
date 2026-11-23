# Zip the Computer Control browser extension as a DEVELOPMENT artifact.
#
# Usage:  pwsh scripts/pack-browser-ext.ps1 [-OutputDirectory <dir>]
#
# The app embeds the extension files and `browser_setup` extracts them for
# "Load unpacked", generating the per-install bootstrap.js credential at that
# time. This zip contains exactly the files the app embeds (no bootstrap.js),
# with the manifest version stamped from Cargo.toml the same way extraction
# does. It is NOT a publishable Web Store package: a store-installed copy cannot
# complete first-pair authentication until store-install pairing exists.

param(
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$src = Join-Path $root 'src/overlay/computer_control/browser_ext'
$setup = Join-Path $root 'src/overlay/computer_control/browser/setup.rs'
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $root 'target/dist' }

# The binary's include_bytes! list is the single source of shipped files.
$files = @(
    [regex]::Matches((Get-Content -LiteralPath $setup -Raw -Encoding UTF8), 'include_bytes!\("\.\./browser_ext/([^"/]+)"\)') |
        ForEach-Object { $_.Groups[1].Value }
)
if ($files -notcontains 'manifest.json') {
    throw "No embedded extension manifest found in $setup"
}

$cargo = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw -Encoding UTF8
$package = [regex]::Match($cargo, '(?ms)^\[package\]\s*$(.*?)(?=^\[)')
$versionMatch = [regex]::Match($package.Groups[1].Value, '(?m)^version\s*=\s*"([^"]+)"')
if (-not $versionMatch.Success) {
    throw 'Cargo.toml [package] version not found'
}
# Chrome wants a plain x.y.z, so drop any -pre/+build suffix (matches extraction).
$version = ($versionMatch.Groups[1].Value -split '[-+]')[0]

$staging = Join-Path ([System.IO.Path]::GetTempPath()) ("sgt-browser-ext-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $staging | Out-Null
try {
    foreach ($file in $files) {
        Copy-Item -LiteralPath (Join-Path $src $file) -Destination (Join-Path $staging $file)
    }
    $manifestPath = Join-Path $staging 'manifest.json'
    $manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $manifest.version = $version
    [System.IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 32), [System.Text.UTF8Encoding]::new($false))

    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    $zip = Join-Path $OutputDirectory ("sgt-browser-bridge-{0}.zip" -f $version)
    if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
    Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $zip
}
finally {
    Remove-Item -LiteralPath $staging -Recurse -Force
}

Write-Host "Packed $($manifest.name) v$version -> $zip"
Write-Warning 'Development artifact only: without store-install pairing, a Web Store install of this zip cannot pair with the app.'
