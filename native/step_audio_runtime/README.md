# Step Audio EditX Managed Runtime

Step Audio EditX uses the official StepFun Python/PyTorch implementation. It is
not a libtorch DLL target.

- Runtime entrypoint: `step-audio-sidecar/step-audio-sidecar.exe`
- Rust caller: `src/api/tts/worker/worker_step_audio.rs`
- Runtime downloader: `src/api/realtime_audio/step_audio_runtime.rs`
- Model downloader: `src/api/realtime_audio/step_audio_assets.rs`
- Upstream code: <https://github.com/stepfun-ai/Step-Audio-EditX>
- Model weights: `stepfun-ai/Step-Audio-EditX-AWQ-4bit`
- Tokenizer weights: `stepfun-ai/Step-Audio-Tokenizer`

The runtime bundle contains a managed Python environment, the upstream
Step-Audio-EditX source, a tiny JSON-lines sidecar, and two built-in prompt
voices copied from the upstream examples. Customer machines do not run `pip`.

Build on Windows:

```powershell
.\native\step_audio_runtime\scripts\build_runtime.ps1
```

Smoke-test the installed AppData runtime and model:

```powershell
.\native\step_audio_runtime\scripts\smoke_runtime.ps1
```

Upload only after local app testing succeeds:

```powershell
.\native\step_audio_runtime\scripts\build_runtime.ps1 -Upload
```

The manifest in `dist/` points at the shared `sgt-runtime-bundles` GitHub
release. Fresh customer runtime download does not work until the matching
`sgt-step-audio-runtime-<version>.zip.part*` assets are uploaded there.
