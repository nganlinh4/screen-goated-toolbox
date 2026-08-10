# VieNeu managed runtime

VieNeu TTS runs as a persistent Python sidecar. The current product catalog
uses the `v2-turbo-gpu` variant, so a working NVIDIA CUDA environment is
required.

The bundle contains a Python 3.11 environment, CUDA PyTorch, the VieNeu SDK,
and `vieneu_sidecar.py`. Model data is cached separately by the SDK under
`%LOCALAPPDATA%\screen-goated-toolbox\models\vieneu_hf`.

The signed host embeds the exact chunk URLs, sizes, SHA-256 values, entrypoint,
and installed size. It reassembles only those verified chunks, validates the
entrypoint and bundled Python, then installs under
`%LOCALAPPDATA%\screen-goated-toolbox\bin\vieneu_runtime`.

The sidecar stays alive and exchanges one JSON object per line. Responses echo
the request `id`; diagnostics use stderr. The host can request a built-in voice
or reference audio, reads the resulting WAV, and rejects empty or silent
output.

## Build and publish

Requirements: Windows, `uv`, `tar.exe`, and optionally `gh` for upload. A
CUDA-capable NVIDIA system is required to validate the current product variant.

```powershell
.\native\vieneu_runtime\scripts\build_runtime.ps1
```

`-SkipInstall` reuses the existing `build/venv`; it is not a clean-build mode.
The script refreshes the local manifest and chunk files in `dist/`.

For publication:

```powershell
$Version = "YYYY.MM.DD"
.\native\vieneu_runtime\scripts\build_runtime.ps1 -Version $Version -SkipInstall -Upload
```

Upload only newly named chunks to the append-only release, verify their remote
sizes and hashes, then update the host's embedded descriptor. The host never
fetches or trusts a manifest at runtime.
