param(
    [string]$Version = (Get-Date -Format "yyyy.MM.dd"),
    [string]$ReleaseTag = "sgt-runtime-bundles",
    [int]$ChunkSizeMb = 1900,
    [switch]$SkipInstall,
    [switch]$Upload
)

$ErrorActionPreference = "Stop"
$RuntimeRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$BuildDir = Join-Path $RuntimeRoot "build"
$VenvDir = Join-Path $BuildDir "venv"
$PackageDir = Join-Path $BuildDir "package"
$SidecarDir = Join-Path $PackageDir "vieneu-sidecar"
$DistDir = Join-Path $RuntimeRoot "dist"
$ArchiveName = "sgt-vieneu-runtime-$Version.zip"
$ArchivePath = Join-Path $DistDir $ArchiveName
$ManifestPath = Join-Path $DistDir "sgt_vieneu_runtime.manifest.json"
$LlamaCppCpuIndex = "https://pnnbao97.github.io/llama-cpp-python-v0.3.16/cpu/"
$TorchCudaIndex = "https://download.pytorch.org/whl/cu128"

function Require-Command($Name) {
    if (!(Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command '$Name' was not found on PATH."
    }
}

function Remove-Path($Path) {
    if (Test-Path $Path) {
        Remove-Item $Path -Recurse -Force
    }
}

function Get-Sha256($Path) {
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $sha = [System.Security.Cryptography.SHA256]::Create()
        try {
            $bytes = $sha.ComputeHash($stream)
            return ([System.BitConverter]::ToString($bytes) -replace "-", "").ToLowerInvariant()
        } finally {
            $sha.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
}

function Split-File($InputPath, $OutputPrefix, [int64]$ChunkBytes) {
    $buffer = New-Object byte[] (4MB)
    $input = [System.IO.File]::OpenRead($InputPath)
    try {
        $index = 1
        while ($input.Position -lt $input.Length) {
            $partPath = "{0}.part{1:D3}" -f $OutputPrefix, $index
            $output = [System.IO.File]::Create($partPath)
            try {
                $written = [int64]0
                while ($written -lt $ChunkBytes) {
                    $toRead = [int][Math]::Min($buffer.Length, $ChunkBytes - $written)
                    $read = $input.Read($buffer, 0, $toRead)
                    if ($read -le 0) { break }
                    $output.Write($buffer, 0, $read)
                    $written += $read
                }
            } finally {
                $output.Dispose()
            }
            $index += 1
        }
    } finally {
        $input.Dispose()
    }
}

Require-Command uv
Require-Command tar.exe
if ($Upload) { Require-Command gh }

New-Item -ItemType Directory -Path $BuildDir, $DistDir -Force | Out-Null
Remove-Path $PackageDir
New-Item -ItemType Directory -Path $SidecarDir -Force | Out-Null

if (!$SkipInstall) {
    Remove-Path $VenvDir
    & uv venv --python 3.11 $VenvDir
    if ($LASTEXITCODE -ne 0) { throw "uv venv failed" }
    $Python = Join-Path $VenvDir "Scripts\python.exe"
    & uv pip install --python $Python --upgrade pip wheel setuptools
    if ($LASTEXITCODE -ne 0) { throw "pip bootstrap failed" }
    & $Python -m pip install `
        --prefer-binary `
        --only-binary llama-cpp-python `
        "llama-cpp-python==0.3.16" `
        --extra-index-url $LlamaCppCpuIndex
    if ($LASTEXITCODE -ne 0) { throw "llama-cpp-python wheel install failed" }
    & $Python -m pip install `
        --no-cache-dir `
        --index-url $TorchCudaIndex `
        "torch==2.10.0+cu128" `
        "torchvision==0.25.0+cu128" `
        "torchaudio==2.10.0+cu128"
    if ($LASTEXITCODE -ne 0) { throw "CUDA PyTorch install failed" }
    & $Python -m pip install `
        --prefer-binary `
        "vieneu[gpu]==2.3.0" `
        --extra-index-url $LlamaCppCpuIndex
    if ($LASTEXITCODE -ne 0) { throw "VieNeu SDK install failed" }
    & $Python -m pip install --prefer-binary "fsspec==2026.2.0"
    if ($LASTEXITCODE -ne 0) { throw "fsspec compatibility pin failed" }
    & $Python -c "import torch; assert torch.version.cuda, f'Expected CUDA torch, got {torch.__version__}'; print(torch.__version__, torch.version.cuda, torch.cuda.is_available())"
    if ($LASTEXITCODE -ne 0) { throw "CUDA PyTorch import check failed" }
    & $Python -c "import vieneu; print('vieneu import ok')"
    if ($LASTEXITCODE -ne 0) { throw "VieNeu import check failed" }
} else {
    $Python = Join-Path $VenvDir "Scripts\python.exe"
    if (!(Test-Path $Python)) {
        throw "SkipInstall was set but '$Python' does not exist."
    }
}

Copy-Item $VenvDir -Destination (Join-Path $SidecarDir "python_runtime") -Recurse -Force
Copy-Item (Join-Path $RuntimeRoot "sidecar\vieneu_sidecar.py") -Destination (Join-Path $SidecarDir "vieneu_sidecar.py") -Force

Remove-Item (Join-Path $DistDir "sgt-vieneu-runtime-*.zip*") -Force -ErrorAction SilentlyContinue
Remove-Item $ArchivePath -Force -ErrorAction SilentlyContinue
Push-Location $PackageDir
try {
    & tar.exe -a -cf $ArchivePath "vieneu-sidecar"
    if ($LASTEXITCODE -ne 0) { throw "tar archive creation failed" }
} finally {
    Pop-Location
}

$chunkPrefix = Join-Path $DistDir $ArchiveName
Split-File $ArchivePath $chunkPrefix ([int64]$ChunkSizeMb * 1024 * 1024)
Remove-Item $ArchivePath -Force

$chunks = Get-ChildItem $DistDir -File -Filter "$ArchiveName.part*" | Sort-Object Name | ForEach-Object {
    [ordered]@{
        filename = $_.Name
        url = "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/$ReleaseTag/$($_.Name)"
        sha256 = Get-Sha256 $_.FullName
        size = $_.Length
    }
}

$installedSize = (Get-ChildItem $SidecarDir -Recurse -File | Measure-Object Length -Sum).Sum
$manifest = [ordered]@{
    version = $Version
    abiVersion = 1
    entrypoint = "vieneu-sidecar/vieneu_sidecar.py"
    installedSize = [int64]$installedSize
    chunks = @($chunks)
}
$manifestJson = $manifest | ConvertTo-Json -Depth 6
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($ManifestPath, $manifestJson, $utf8NoBom)

if ($Upload) {
    $releaseExists = $true
    & gh release view $ReleaseTag *> $null
    if ($LASTEXITCODE -ne 0) { $releaseExists = $false }
    if (!$releaseExists) {
        & gh release create $ReleaseTag --title "SGT Runtime Bundles" --notes "Runtime bundles for downloadable local engines."
        if ($LASTEXITCODE -ne 0) { throw "gh release create failed" }
    }
    $assets = @($ManifestPath) + @(Get-ChildItem $DistDir -File -Filter "$ArchiveName.part*" | ForEach-Object { $_.FullName })
    & gh release upload $ReleaseTag @assets --clobber
    if ($LASTEXITCODE -ne 0) { throw "gh release upload failed" }
}

Write-Host "VieNeu runtime manifest: $ManifestPath"
Write-Host "VieNeu runtime chunks:"
Get-ChildItem $DistDir -File -Filter "$ArchiveName.part*" | Sort-Object Name | ForEach-Object {
    Write-Host ("  {0} ({1:n0} bytes)" -f $_.Name, $_.Length)
}
