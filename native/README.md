# Native Runtimes

SGT-owned Windows runtime crates, sidecars, packaging scripts, and manifests live here. The desktop host remains under `src/`; model/install metadata remains in `catalog/model_catalog.json`.

## Active or Packaged Paths

- [Qwen3-ASR DLL](qwen3_runtime/README.md) — active CUDA transcription backend.
- [Qwen3 reference sidecar](qwen3_reference_sidecar/README.md) — diagnostic/settings-managed executable, not the active ASR backend.
- [Step Audio EditX](step_audio_runtime/README.md) — managed persistent Python sidecar.
- [NVIDIA Magpie](magpie_runtime/README.md) — managed persistent Python sidecar.
- [VieNeu](vieneu_runtime/README.md) — managed persistent Python sidecar.

## Incomplete Path

- [Voxtral](voxtral_runtime/README.md) — configured integration without a committed publishable runtime DLL. Do not present clean installation as working.
- [Shared TTS DLL ABI](README_TTS_RUNTIME_FFI.md) — host ABI and prototype status.

## Rules

- Keep build, package, manifest, catalog, installer, and host-loader claims aligned.
- Do not call a runtime active because a prototype source tree exists; verify the shipped artifact and clean-install URL.
- Never commit model weights, credentials, machine paths, or generated environments.
- Validate the exact host protocol and a real inference request before publishing.
