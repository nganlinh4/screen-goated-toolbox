param(
    [string]$Version = (Get-Date -Format "yyyy.MM.dd"),
    [string]$ReleaseTag = "sgt-runtime-bundles",
    [int]$ChunkSizeMb = 1900,
    [switch]$SkipInstall,
    [switch]$Upload
)

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")
$RuntimeRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$BuildDir = Join-Path $RuntimeRoot "build"
$VenvDir = Join-Path $BuildDir "venv"
$PackageDir = Join-Path $BuildDir "package"
$SidecarDir = Join-Path $PackageDir "magpie-sidecar"
$DistDir = Join-Path $RuntimeRoot "dist"
$ArchiveName = "sgt-magpie-runtime-$Version.zip"
$ArchivePath = Join-Path $DistDir $ArchiveName
$ManifestPath = Join-Path $DistDir "sgt_magpie_runtime.manifest.json"

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
    if (Get-Command Get-FileHash -ErrorAction SilentlyContinue) {
        return (Get-FileHash -Algorithm SHA256 $Path).Hash.ToLowerInvariant()
    }
    $hashLine = & certutil.exe -hashfile $Path SHA256 |
        Where-Object { $_ -match '^[0-9a-fA-F ]+$' } |
        Select-Object -First 1
    if ($LASTEXITCODE -ne 0 -or !$hashLine) {
        throw "Failed to calculate SHA256 for $Path"
    }
    ($hashLine -replace '\s', '').ToLowerInvariant()
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
Require-Command py
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
    & uv pip install --python $Python --index-url "https://download.pytorch.org/whl/cu128" torch
    if ($LASTEXITCODE -ne 0) { throw "PyTorch install failed" }
    & uv pip install --python $Python Cython
    if ($LASTEXITCODE -ne 0) { throw "Cython install failed" }
    & uv pip install --python $Python `
        nemo_toolkit `
        "lightning==2.5.0" `
        "pytorch-lightning==2.5.0" `
        hydra-core `
        omegaconf `
        einops `
        librosa `
        soundfile `
        jiwer `
        inflect `
        unidecode `
        wrapt `
        tqdm `
        matplotlib `
        pandas `
        transformers `
        sentencepiece `
        kornia `
        pyarrow `
        wandb `
        resampy `
        janome `
        pypinyin-dict `
        lxml `
        pyinstaller `
        kaldialign `
        braceexpand `
        webdataset `
        kaldi-python-io `
        sox `
        fiddle `
        cloudpickle `
        nv-one-logger-core `
        nv-one-logger-training-telemetry `
        nv-one-logger-pytorch-lightning-integration `
        lhotse `
        pyannote.core `
        pyannote.metrics `
        datasets `
        editdistance `
        ipython `
        jieba `
        pyopenjtalk
    if ($LASTEXITCODE -ne 0) { throw "NeMo runtime dependency install failed" }
    & uv pip install --python $Python --index-url "https://download.pytorch.org/whl/cu128" --reinstall torch
    if ($LASTEXITCODE -ne 0) { throw "CUDA PyTorch reinstall failed" }
    & $Python -c "import torch; assert torch.version.cuda, f'Expected CUDA PyTorch, got {torch.__version__}'"
    if ($LASTEXITCODE -ne 0) { throw "CUDA PyTorch import check failed" }
    & $Python -c "from nemo.collections.tts.models import MagpieTTSModel; print('MagpieTTSModel import ok')"
    if ($LASTEXITCODE -ne 0) { throw "MagpieTTSModel import check failed" }
} else {
    $Python = Join-Path $VenvDir "Scripts\python.exe"
    if (!(Test-Path $Python)) {
        throw "SkipInstall was set but '$Python' does not exist."
    }
}

$Launcher = Join-Path $RuntimeRoot "launcher\magpie_launcher.py"
$PyinstallerDist = Join-Path $BuildDir "pyinstaller-dist"
$PyinstallerWork = Join-Path $BuildDir "pyinstaller-work"
Remove-Path $PyinstallerDist
Remove-Path $PyinstallerWork
& $Python -m PyInstaller --clean --onefile --name magpie-sidecar --distpath $PyinstallerDist --workpath $PyinstallerWork $Launcher
if ($LASTEXITCODE -ne 0) { throw "PyInstaller failed" }

Copy-Item (Join-Path $PyinstallerDist "magpie-sidecar.exe") -Destination (Join-Path $SidecarDir "magpie-sidecar.exe") -Force
Copy-Item $VenvDir -Destination (Join-Path $SidecarDir "python_runtime") -Recurse -Force
New-Item -ItemType Directory -Path (Join-Path $SidecarDir "sidecar") -Force | Out-Null
Copy-Item (Join-Path $RuntimeRoot "sidecar\magpie_sidecar.py") -Destination (Join-Path $SidecarDir "sidecar\magpie_sidecar.py") -Force

Remove-Item (Join-Path $DistDir "sgt-magpie-runtime-*.zip*") -Force -ErrorAction SilentlyContinue
Remove-Item $ArchivePath -Force -ErrorAction SilentlyContinue
Push-Location $PackageDir
try {
    & tar.exe -a -cf $ArchivePath "magpie-sidecar"
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
    entrypoint = "magpie-sidecar/magpie-sidecar.exe"
    installedSize = [int64]$installedSize
    chunks = @($chunks)
}
$manifest | ConvertTo-Json -Depth 6 | Set-Content -Path $ManifestPath -Encoding UTF8

if ($Upload) {
    $releaseExists = $true
    & gh release view $ReleaseTag *> $null
    if ($LASTEXITCODE -ne 0) { $releaseExists = $false }
    if (!$releaseExists) {
        & gh release create $ReleaseTag --title "SGT Runtime Bundles" --notes "Runtime bundles for downloadable local engines."
        if ($LASTEXITCODE -ne 0) { throw "gh release create failed" }
    }
    $assets = @(Get-ChildItem $DistDir -File -Filter "$ArchiveName.part*" | ForEach-Object { $_.FullName })
    & gh release upload $ReleaseTag @assets --clobber
    if ($LASTEXITCODE -ne 0) { throw "gh release upload failed" }
}

Write-Host "Magpie runtime manifest: $ManifestPath"
Write-Host "Magpie runtime chunks:"
Get-ChildItem $DistDir -File -Filter "$ArchiveName.part*" | Sort-Object Name | ForEach-Object {
    Write-Host ("  {0} ({1:n0} bytes)" -f $_.Name, $_.Length)
}
