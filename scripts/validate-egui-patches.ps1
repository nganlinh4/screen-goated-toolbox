$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$contractPath = Join-Path $PSScriptRoot "egui-patch-contract.ps1"
$contract = & $contractPath

function Invoke-DependencyGit {
    param(
        [Parameter(Mandatory = $true)][string]$Directory,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    $previousPreference = $ErrorActionPreference
    try {
        # Windows PowerShell promotes native stderr to ErrorRecord objects under Stop. Capture it
        # as ordinary diagnostic output so the Git exit code remains the structural result.
        $ErrorActionPreference = "Continue"
        $output = @(& git -C $Directory @Arguments 2>&1)
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($exitCode -ne 0) {
        throw "git -C $Directory $($Arguments -join ' ') failed:`n$($output -join [Environment]::NewLine)"
    }
    return $output
}

foreach ($dependency in $contract.Dependencies) {
    $directory = Join-Path $repoRoot $dependency.RelativeDirectory
    if (-not (Test-Path -LiteralPath (Join-Path $directory ".git"))) {
        throw "$($dependency.Name) is not initialized; run scripts/setup-egui-snarl.ps1"
    }

    $head = (Invoke-DependencyGit -Directory $directory -Arguments @("rev-parse", "HEAD") |
        Select-Object -Last 1).Trim()
    if ($head -ne $dependency.Revision) {
        throw "$($dependency.Name) is at $head instead of pinned revision $($dependency.Revision)"
    }

    $temporaryIndex = Join-Path ([System.IO.Path]::GetTempPath()) (
        "sgt-$($dependency.Name)-patch-$([guid]::NewGuid().ToString('N')).index"
    )
    $previousIndex = [Environment]::GetEnvironmentVariable("GIT_INDEX_FILE", "Process")

    try {
        $env:GIT_INDEX_FILE = $temporaryIndex
        Invoke-DependencyGit -Directory $directory -Arguments @("read-tree", $dependency.Revision) | Out-Null
        foreach ($relativePatch in $dependency.Patches) {
            $patchPath = Join-Path $repoRoot $relativePatch
            if (-not (Test-Path -LiteralPath $patchPath -PathType Leaf)) {
                throw "Missing tracked patch: $relativePatch"
            }
            Invoke-DependencyGit -Directory $directory -Arguments @(
                "apply", "--cached", "--whitespace=nowarn", $patchPath
            ) | Out-Null
        }
        $expectedTree = (Invoke-DependencyGit -Directory $directory -Arguments @("write-tree") |
            Select-Object -Last 1).Trim()

        Remove-Item -LiteralPath $temporaryIndex -Force
        Invoke-DependencyGit -Directory $directory -Arguments @("read-tree", $dependency.Revision) | Out-Null
        Invoke-DependencyGit -Directory $directory -Arguments @("add", "--all", "--", ".") | Out-Null
        $actualTree = (Invoke-DependencyGit -Directory $directory -Arguments @("write-tree") |
            Select-Object -Last 1).Trim()
    }
    finally {
        if ([string]::IsNullOrEmpty($previousIndex)) {
            Remove-Item Env:GIT_INDEX_FILE -ErrorAction SilentlyContinue
        }
        else {
            $env:GIT_INDEX_FILE = $previousIndex
        }
        Remove-Item -LiteralPath $temporaryIndex -Force -ErrorAction SilentlyContinue
    }

    if ($actualTree -ne $expectedTree) {
        throw "$($dependency.Name) contains changes not reproduced by its tracked patches; edit the patch files and rerun scripts/setup-egui-snarl.ps1"
    }
}

Write-Host "Pinned egui dependency checkouts exactly match their tracked patches."
