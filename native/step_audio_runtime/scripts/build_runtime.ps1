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
$SidecarDir = Join-Path $PackageDir "step-audio-sidecar"
$DistDir = Join-Path $RuntimeRoot "dist"
$ArchiveName = "sgt-step-audio-runtime-$Version.zip"
$ArchivePath = Join-Path $DistDir $ArchiveName
$ManifestPath = Join-Path $DistDir "sgt_step_audio_runtime.manifest.json"
$UpstreamDir = Join-Path $BuildDir "Step-Audio-EditX"

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
    try {
        $sha = [System.Security.Cryptography.SHA256]::Create()
        $stream = [System.IO.File]::OpenRead($Path)
        try {
            $bytes = $sha.ComputeHash($stream)
            return (($bytes | ForEach-Object { $_.ToString("x2") }) -join "")
        } finally {
            $stream.Dispose()
            $sha.Dispose()
        }
    } catch {
        throw "Failed to calculate SHA256 for $Path"
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
Require-Command py
Require-Command git
Require-Command tar.exe
if ($Upload) { Require-Command gh }

New-Item -ItemType Directory -Path $BuildDir, $DistDir -Force | Out-Null
Remove-Path $PackageDir
New-Item -ItemType Directory -Path $SidecarDir -Force | Out-Null

if (!(Test-Path $UpstreamDir)) {
    & git clone --depth 1 https://github.com/stepfun-ai/Step-Audio-EditX.git $UpstreamDir
    if ($LASTEXITCODE -ne 0) { throw "Step-Audio-EditX clone failed" }
}

if (!$SkipInstall) {
    Remove-Path $VenvDir
    & uv venv --python 3.12 $VenvDir
    if ($LASTEXITCODE -ne 0) { throw "uv venv failed" }
    $Python = Join-Path $VenvDir "Scripts\python.exe"
    & uv pip install --python $Python --upgrade pip wheel setuptools
    if ($LASTEXITCODE -ne 0) { throw "pip bootstrap failed" }
    & uv pip install --python $Python --index-url "https://download.pytorch.org/whl/cu128" torch torchaudio torchvision
    if ($LASTEXITCODE -ne 0) { throw "CUDA PyTorch install failed" }
    & uv pip install --python $Python `
        "transformers==4.57.3" `
        "huggingface-hub<1.0" `
        "accelerate>=1.10.1" `
        "compressed-tensors>=0.11.0" `
        sentencepiece `
        "onnxruntime-gpu>=1.23.2" `
        "openai-whisper>=20250625" `
        "funasr>=1.3.0" `
        "librosa>=0.11.0" `
        soundfile `
        "sox>=1.5.0" `
        "hyperpyyaml>=1.2.3" `
        "conformer>=0.3.2" `
        "torch-complex>=0.4.4" `
        "rotary-embedding-torch>=0.8.9" `
        einops `
        numpy `
        scipy `
        pyyaml `
        pyinstaller
    if ($LASTEXITCODE -ne 0) { throw "Step Audio runtime dependency install failed" }
    & $Python -c "import torch; assert torch.version.cuda, f'Expected CUDA PyTorch, got {torch.__version__}'"
    if ($LASTEXITCODE -ne 0) { throw "CUDA PyTorch import check failed" }
    & $Python -c "import transformers, onnxruntime, whisper, funasr; print('Step Audio imports ok')"
    if ($LASTEXITCODE -ne 0) { throw "Step Audio import check failed" }
} else {
    $Python = Join-Path $VenvDir "Scripts\python.exe"
    if (!(Test-Path $Python)) {
        throw "SkipInstall was set but '$Python' does not exist."
    }
}

$Launcher = Join-Path $RuntimeRoot "launcher\step_audio_launcher.py"
$PyinstallerDist = Join-Path $BuildDir "pyinstaller-dist"
$PyinstallerWork = Join-Path $BuildDir "pyinstaller-work"
Remove-Path $PyinstallerDist
Remove-Path $PyinstallerWork
& $Python -m PyInstaller --clean --onefile --name step-audio-sidecar --distpath $PyinstallerDist --workpath $PyinstallerWork $Launcher
if ($LASTEXITCODE -ne 0) { throw "PyInstaller failed" }

Copy-Item (Join-Path $PyinstallerDist "step-audio-sidecar.exe") -Destination (Join-Path $SidecarDir "step-audio-sidecar.exe") -Force
Copy-Item $VenvDir -Destination (Join-Path $SidecarDir "python_runtime") -Recurse -Force
New-Item -ItemType Directory -Path (Join-Path $SidecarDir "sidecar") -Force | Out-Null
Copy-Item (Join-Path $RuntimeRoot "sidecar\step_audio_sidecar.py") -Destination (Join-Path $SidecarDir "sidecar\step_audio_sidecar.py") -Force
New-Item -ItemType Directory -Path (Join-Path $SidecarDir "upstream") -Force | Out-Null
Copy-Item $UpstreamDir -Destination (Join-Path $SidecarDir "upstream\Step-Audio-EditX") -Recurse -Force
Remove-Path (Join-Path $SidecarDir "upstream\Step-Audio-EditX\.git")
New-Item -ItemType Directory -Path (Join-Path $SidecarDir "prompts") -Force | Out-Null
Copy-Item (Join-Path $UpstreamDir "examples\zero_shot_en_prompt.wav") -Destination (Join-Path $SidecarDir "prompts\zero_shot_en_prompt.wav") -Force
Copy-Item (Join-Path $UpstreamDir "examples\fear_zh_female_prompt.wav") -Destination (Join-Path $SidecarDir "prompts\fear_zh_female_prompt.wav") -Force

Remove-Item (Join-Path $DistDir "sgt-step-audio-runtime-*.zip*") -Force -ErrorAction SilentlyContinue
Remove-Item $ArchivePath -Force -ErrorAction SilentlyContinue
Push-Location $PackageDir
try {
    & tar.exe -a -cf $ArchivePath "step-audio-sidecar"
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
    entrypoint = "step-audio-sidecar/step-audio-sidecar.exe"
    installedSize = [int64]$installedSize
    chunks = @($chunks)
}
$manifestJson = $manifest | ConvertTo-Json -Depth 6
[System.IO.File]::WriteAllText(
    $ManifestPath,
    $manifestJson,
    [System.Text.UTF8Encoding]::new($false)
)

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

Write-Host "Step Audio runtime manifest: $ManifestPath"
Write-Host "Step Audio runtime chunks:"
Get-ChildItem $DistDir -File -Filter "$ArchiveName.part*" | Sort-Object Name | ForEach-Object {
    Write-Host ("  {0} ({1:n0} bytes)" -f $_.Name, $_.Length)
}
