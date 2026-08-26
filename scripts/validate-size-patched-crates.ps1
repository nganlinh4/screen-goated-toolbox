# Validate disposable crates against exact source and patch contracts.

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$contracts = & (Join-Path $PSScriptRoot "size-patched-crate-contract.ps1")

function Get-Sha256 {
    param([Parameter(Mandatory = $true)][string]$Path)

    $stream = [System.IO.File]::OpenRead($Path)
    $hasher = [Security.Cryptography.SHA256]::Create()
    try {
        ([BitConverter]::ToString($hasher.ComputeHash($stream))).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $hasher.Dispose()
        $stream.Dispose()
    }
}

foreach ($contract in $contracts) {
    $directory = Join-Path $repoRoot $contract.RelativeDirectory
    if (-not (Test-Path -LiteralPath $directory -PathType Container)) {
        throw "$($contract.Name) is not initialized; run scripts/setup-egui-snarl.ps1"
    }
    $marker = Join-Path $directory ".sgt-source-sha256"
    if (-not (Test-Path -LiteralPath $marker -PathType Leaf) -or
        (Get-Content -Raw -LiteralPath $marker).Trim() -ne $contract.ArchiveSha256) {
        throw "$($contract.Name) does not come from the pinned crates.io archive."
    }

    $cargoToml = Get-Content -Raw -LiteralPath (Join-Path $directory "Cargo.toml")
    if (-not $cargoToml.Contains($contract.RequiredText) -or
        (-not [string]::IsNullOrEmpty($contract.ForbiddenText) -and
            $cargoToml.Contains($contract.ForbiddenText))) {
        throw "$($contract.Name) does not contain its tracked size patch."
    }

    $records = [Collections.Generic.List[string]]::new()
    $rootPrefix = [System.IO.Path]::GetFullPath($directory).TrimEnd('\') + '\'
    Get-ChildItem -LiteralPath $directory -Recurse -File |
        Where-Object { $_.Name -ne ".sgt-source-sha256" } |
        ForEach-Object {
            $fullPath = [System.IO.Path]::GetFullPath($_.FullName)
            if (-not $fullPath.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
                throw "$fullPath escaped the patched crate root $directory"
            }
            $relative = $fullPath.Substring($rootPrefix.Length).Replace('\', '/')
            $hash = Get-Sha256 -Path $_.FullName
            [void]$records.Add("$relative`0$hash`n")
        }
    $records.Sort([StringComparer]::Ordinal)
    $payload = [Text.Encoding]::UTF8.GetBytes(($records -join ""))
    $hasher = [Security.Cryptography.SHA256]::Create()
    try {
        $actualTreeSha256 = ([BitConverter]::ToString($hasher.ComputeHash($payload))).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $hasher.Dispose()
    }
    if ($contract.PatchedTreeSha256 -eq "PENDING") {
        Write-Host "$($contract.Name) patched tree SHA-256: $actualTreeSha256"
    }
    elseif ($actualTreeSha256 -ne $contract.PatchedTreeSha256) {
        throw "$($contract.Name) patched tree hash mismatch: expected $($contract.PatchedTreeSha256), got $actualTreeSha256"
    }
}
