# Magpie managed runtime

NVIDIA Magpie-Multilingual 357M runs as a persistent Python/NeMo sidecar. It is
not part of the shared TTS DLL ABI.

## Package boundary

The runtime bundle contains Python 3.11, CUDA PyTorch, NeMo dependencies, a
small frozen launcher, and `sidecar/magpie_sidecar.py`. The app downloads the
Magpie and NanoCodec `.nemo` checkpoints separately from Hugging Face, with a
ModelScope fallback.

The app fetches
`dist/sgt_magpie_runtime.manifest.json` from the repository's `main` branch,
then downloads, size-checks, SHA-256-checks, joins, and extracts the chunks
listed there. The manifest is the authority for the current version and
filenames.

## Sidecar protocol

The process stays alive and exchanges one JSON object per line. Requests and
responses carry the same `id`; diagnostics use stderr. A request is shaped as:

```json
{
  "id": "123",
  "text": "Hello",
  "language": "en",
  "voice": "Sofia",
  "speed": 1.0,
  "magpieModelPath": "C:/.../magpie_tts_multilingual_357m.nemo",
  "codecModelPath": "C:/.../nemo-nano-codec-22khz-1.89kbps-21.5fps.nemo",
  "outputWavPath": "C:/.../magpie.wav"
}
```

Success returns `id`, `ok`, `sampleRate`, and `outputWavPath`. Failure returns
`id`, `ok: false`, and `error`. The Rust host verifies response correlation and
reads the generated WAV.

`speed` is currently serialized by the host but not applied by the sidecar.
Do not document speed control as supported until inference uses it.

## Build and publish

Requirements: Windows, an NVIDIA CUDA-capable system, `uv`, the Windows Python
launcher (`py`), `tar.exe`, and optionally `gh` for upload.

```powershell
.\native\magpie_runtime\scripts\build_runtime.ps1
```

The script writes chunk files and refreshes the committed manifest in `dist/`.
`-SkipInstall` reuses the existing `build/venv`; it is not a clean-build mode.
`-Upload` uploads chunks to the selected GitHub release, but the refreshed
manifest still must be committed and pushed because the app reads it from
`main`.
