# Recreate disposable egui dependency checkouts from pinned revisions and tracked patches.

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$contract = & (Join-Path $PSScriptRoot "egui-patch-contract.ps1")

function Initialize-PinnedCheckout {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Directory,
        [Parameter(Mandatory = $true)][string]$Repository,
        [switch]$Shallow
    )

    if (Test-Path -LiteralPath $Directory) {
        if (Test-Path -LiteralPath (Join-Path $Directory ".git")) {
            return
        }
        $children = @(Get-ChildItem -LiteralPath $Directory -Force)
        if ($children.Count -ne 0) {
            Write-Error "$Name path exists but is not a Git checkout: $Directory"
            exit 1
        }
        Remove-Item -LiteralPath $Directory
    }

    Write-Host "Cloning $Name..."
    $arguments = @("clone")
    if ($Shallow) {
        $arguments += @("--depth", "20")
    }
    $arguments += @($Repository, $Directory)
    git @arguments
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to clone $Name."
        exit 1
    }
}

foreach ($dependency in $contract.Dependencies) {
    $directory = Join-Path $repoRoot $dependency.RelativeDirectory
    Initialize-PinnedCheckout `
        -Name $dependency.Name `
        -Directory $directory `
        -Repository $dependency.Repository `
        -Shallow:$dependency.Shallow

    Write-Host "Checking out $($dependency.Name) revision $($dependency.Revision)..."
    git -C $directory checkout --force $dependency.Revision
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to checkout $($dependency.Name) revision $($dependency.Revision)."
    }

    foreach ($relativePatch in $dependency.Patches) {
        $patchPath = Join-Path $repoRoot $relativePatch
        if (-not (Test-Path -LiteralPath $patchPath -PathType Leaf)) {
            throw "Missing tracked patch: $relativePatch"
        }
        Write-Host "Applying $relativePatch..."
        git -C $directory apply --whitespace=nowarn $patchPath
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to apply $relativePatch to $($dependency.Name)."
        }
    }
}

& (Join-Path $PSScriptRoot "validate-egui-patches.ps1")
Write-Host "Patched egui dependencies are ready."
