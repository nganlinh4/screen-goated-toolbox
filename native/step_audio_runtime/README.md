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

The signed host embeds the exact runtime version, entrypoint, installed size,
and chunk URLs/sizes/SHA-256 values. The tracked `dist` manifest is packaging
input only; installation never trusts a runtime-fetched manifest.

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
Upload only newly named chunks to the append-only `sgt-runtime-bundles`
release, verify their GitHub-recorded size and SHA-256, and update the host's
embedded descriptor. Never replace or delete bytes referenced by a released
host.

Run the full checklist before publishing:
[`TESTING.md`](TESTING.md).

Relevant host code:

- `src/api/tts/worker/worker_step_audio.rs`
- `src/api/realtime_audio/step_audio_runtime.rs`
- `src/api/realtime_audio/step_audio_assets.rs`
