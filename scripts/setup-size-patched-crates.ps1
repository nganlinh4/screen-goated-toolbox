# Recreate size-patched crates from exact crates.io bytes.

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$contracts = & (Join-Path $PSScriptRoot "size-patched-crate-contract.ps1")

foreach ($contract in $contracts) {
    $destination = Join-Path $repoRoot $contract.RelativeDirectory
    if (Test-Path -LiteralPath $destination) {
        continue
    }
    $work = Join-Path ([System.IO.Path]::GetTempPath()) `
        ("sgt-$($contract.Name)-" + [guid]::NewGuid().ToString("N"))
    $archive = Join-Path $work "$($contract.Name).crate"
    $extract = Join-Path $work "extract"
    New-Item -ItemType Directory -Path $extract -Force | Out-Null

    try {
        $url = "https://static.crates.io/crates/$($contract.Name)/$($contract.Name)-$($contract.Version).crate"
        Invoke-WebRequest -Uri $url -OutFile $archive -UseBasicParsing
        $actualSha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actualSha256 -ne $contract.ArchiveSha256) {
            throw "$($contract.Name) archive hash mismatch: expected $($contract.ArchiveSha256), got $actualSha256"
        }

        & tar.exe -xf $archive -C $extract
        if ($LASTEXITCODE -ne 0) { throw "Failed to extract $($contract.Name)." }
        $source = Join-Path $extract "$($contract.Name)-$($contract.Version)"
        if (-not (Test-Path -LiteralPath $source -PathType Container)) {
            throw "$($contract.Name) archive has an unexpected layout."
        }

        New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
        Move-Item -LiteralPath $source -Destination $destination
        $patch = Join-Path $repoRoot $contract.Patch
        & git -C $repoRoot apply --unsafe-paths `
            "--directory=$($contract.RelativeDirectory)" --whitespace=nowarn $patch
        if ($LASTEXITCODE -ne 0) { throw "Failed to apply $($contract.Patch)." }
        Set-Content -LiteralPath (Join-Path $destination ".sgt-source-sha256") `
            -Value $contract.ArchiveSha256 -NoNewline -Encoding Ascii
    }
    catch {
        if (Test-Path -LiteralPath $destination) {
            Remove-Item -LiteralPath $destination -Recurse -Force
        }
        throw
    }
    finally {
        if (Test-Path -LiteralPath $work) {
            Remove-Item -LiteralPath $work -Recurse -Force
        }
    }
}
