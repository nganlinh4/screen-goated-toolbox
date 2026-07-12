# Step Audio EditX managed runtime

Step Audio EditX runs through a persistent managed Python/PyTorch sidecar. It
does not use the shared TTS DLL ABI.

## Package boundary

The runtime bundle contains:

- a Python 3.12 environment with CUDA PyTorch and inference dependencies;
- the upstream Step-Audio-EditX source;
- `step-audio-sidecar/step-audio-sidecar.exe` and its Python implementation;
- two prompt WAV files copied from upstream examples.

The AWQ model and tokenizer are separate app-managed downloads. End-user
machines do not run `pip`.

The app fetches `dist/sgt_step_audio_runtime.manifest.json` from the
repository's `main` branch and verifies every chunk's size and SHA-256 before
installation. The manifest is the authority for the current version and
filenames.

The sidecar stays alive and exchanges one JSON object per line over
stdin/stdout. Responses echo the request `id`; diagnostics use stderr. It
supports clone/TTS requests and audio-edit operations, writes a mono WAV to the
requested path, and reports its sample rate.

## Build and publish

Requirements: Windows, an NVIDIA CUDA-capable system, `uv`, the Windows Python
launcher (`py`), `git`, `tar.exe`, and optionally `gh` for upload.

```powershell
.\native\step_audio_runtime\scripts\build_runtime.ps1
```

`-SkipInstall` reuses the existing `build/venv`; it is not a clean-build mode.
`-Upload` uploads chunk files to the selected GitHub release. The refreshed
manifest must still be committed and pushed because the app reads it from
`main`.

Run the full checklist before publishing:
[`TESTING.md`](TESTING.md).

Relevant host code:

- `src/api/tts/worker/worker_step_audio.rs`
- `src/api/realtime_audio/step_audio_runtime.rs`
- `src/api/realtime_audio/step_audio_assets.rs`
