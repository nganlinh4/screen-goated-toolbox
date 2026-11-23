# Dot-source to define Initialize-PinnedCheckout for disposable pinned dependency checkouts.

function Test-PinnedRevision {
    param(
        [Parameter(Mandatory = $true)][string]$Directory,
        [Parameter(Mandatory = $true)][string]$Revision
    )

    git -C $Directory rev-parse --verify --quiet "$Revision^{commit}" | Out-Null
    return $LASTEXITCODE -eq 0
}

function Initialize-PinnedCheckout {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Directory,
        [Parameter(Mandatory = $true)][string]$Repository,
        [Parameter(Mandatory = $true)][string]$Revision,
        [switch]$Shallow
    )

    if (Test-Path -LiteralPath $Directory) {
        if (-not (Test-Path -LiteralPath (Join-Path $Directory ".git"))) {
            $children = @(Get-ChildItem -LiteralPath $Directory -Force)
            if ($children.Count -ne 0) {
                throw "$Name path exists but is not a Git checkout: $Directory"
            }
            Remove-Item -LiteralPath $Directory
        }
    }

    if (-not (Test-Path -LiteralPath $Directory)) {
        Write-Host "Cloning $Name..."
        $arguments = @("clone", "--no-checkout")
        if ($Shallow) {
            $arguments += @("--depth", "1")
        }
        $arguments += @($Repository, $Directory)
        git @arguments
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to clone $Name."
        }
    }

    # A clone follows the moving default branch, so a shallow history may not
    # contain the pinned revision. Fetch that exact commit when it is missing.
    if (-not (Test-PinnedRevision -Directory $Directory -Revision $Revision)) {
        Write-Host "Fetching $Name revision $Revision..."
        $fetch = @("-C", $Directory, "fetch")
        if ($Shallow) {
            $fetch += @("--depth", "1")
        }
        $fetch += @($Repository, $Revision)
        git @fetch
        if ($LASTEXITCODE -ne 0 -or -not (Test-PinnedRevision -Directory $Directory -Revision $Revision)) {
            throw "Failed to fetch $Name revision $Revision."
        }
    }

    Write-Host "Checking out $Name revision $Revision..."
    git -C $Directory checkout --force $Revision
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to checkout $Name revision $Revision."
    }
}
