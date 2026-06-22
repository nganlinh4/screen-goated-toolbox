# Package the Computer Control browser extension for distribution / Chrome Web
# Store upload. Zips the unpacked extension into target/dist/.
#
# Usage:  pwsh scripts/pack-browser-ext.ps1
#
# Day-to-day the extension does NOT need this: the app ships the files inside the
# binary and `browser_setup` extracts them to disk for "Load unpacked". This zip
# is only for publishing to the Web Store (one-click install, auto-update, and a
# stable extension ID assigned by the store).

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$src = Join-Path $root 'src/overlay/computer_control/browser_ext'
$out = Join-Path $root 'target/dist'
New-Item -ItemType Directory -Force -Path $out | Out-Null

$manifest = Get-Content (Join-Path $src 'manifest.json') -Raw | ConvertFrom-Json
$zip = Join-Path $out ("sgt-browser-bridge-{0}.zip" -f $manifest.version)
if (Test-Path $zip) { Remove-Item $zip -Force }

Compress-Archive -Path (Join-Path $src '*') -DestinationPath $zip
Write-Host "Packed $($manifest.name) v$($manifest.version) -> $zip"
Write-Host "Upload at https://chrome.google.com/webstore/devconsole (the store assigns the stable extension ID)."
