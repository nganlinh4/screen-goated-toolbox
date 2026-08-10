# Native Runtimes

SGT-owned Windows runtime crates, sidecars, packaging scripts, and manifests live here. The desktop host remains under `src/`; model/install metadata remains in `catalog/model_catalog.json`.

## Active or Packaged Paths

- [Qwen3-ASR DLL](qwen3_runtime/README.md) — active CUDA transcription backend.
- [Qwen3 reference sidecar](qwen3_reference_sidecar/README.md) — archived
  diagnostic source; it is not shipped or exposed by the app.
- [Step Audio EditX](step_audio_runtime/README.md) — managed persistent Python sidecar.
- [NVIDIA Magpie](magpie_runtime/README.md) — managed persistent Python sidecar.
- [VieNeu](vieneu_runtime/README.md) — managed persistent Python sidecar.
- [Computer Control engine](computer_control_engine/README.md) — removable x64
  data-only planning and provider-protocol worker.
- `language_catalog/` — complete compact ISO 639 lookup crate shared by the
  signed host and recorder worker; exhaustive tests preserve upstream mappings.

## Rules

- Keep build, package, manifest, catalog, installer, and host-loader claims aligned.
- Do not call a runtime active because a prototype source tree exists; verify the shipped artifact and clean-install URL.
- Never commit model weights, credentials, machine paths, or generated environments.
- Validate the exact host protocol and a real inference request before publishing.
