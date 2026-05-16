# Step Audio EditX Test Checklist

Use this checklist before uploading `sgt-step-audio-runtime-*.zip.part*` to the
`sgt-runtime-bundles` GitHub release.

## 1. Runtime Smoke

```powershell
cd C:\WORK\screen-goated-toolbox
.\native\step_audio_runtime\scripts\smoke_runtime.ps1
```

Expected:
- JSON response contains `"ok": true`.
- Output WAV is at least 0.45 seconds long.
- The script prints the WAV path and byte size.

## 2. Desktop App

Before manual UI checks, run the gated Rust e2e checks against the installed
runtime/model:

```powershell
cd C:\WORK\screen-goated-toolbox
$env:SGT_STEP_AUDIO_E2E = "1"
cargo test --target x86_64-pc-windows-gnu api::tts::worker::worker_step_audio::tests::synthesize_step_audio_e2e_when_enabled -- --nocapture
$env:SGT_STEP_AUDIO_MANAGER_E2E = "1"
cargo test --target x86_64-pc-windows-gnu api::tts::manager::tests::synthesize_to_wav_with_step_audio_profile_e2e_when_enabled -- --nocapture
```

Expected:
- Worker e2e produces at least 0.5 seconds of 24 kHz audio.
- Manager e2e produces a valid WAV artifact through the same collection path
  used by TTS Playground and subtitle narration.

```powershell
cd C:\WORK\screen-goated-toolbox
.\scripts\run_desktop_dev.ps1
```

Expected:
- App starts without a Step Audio startup error.
- `Settings -> Downloaded Tools` shows separate Step Audio rows for:
  - AWQ model + tokenizer
  - managed runtime
- Selecting Step Audio EditX for Read Aloud produces audible speech.
- Tiny/empty audio is not treated as success; the sidecar retries or reports an
  error.

## 3. Screen-Record Narration

Expected:
- Narration method list includes Step Audio EditX.
- Step Audio shows prompt voice rows per language.
- Prompt text override is not visible for bundled prompt voices.
- Generating narration with Step Audio creates audible narration clips.

## 4. Release Upload

Only after the app checks pass:

```powershell
cd C:\WORK\screen-goated-toolbox
.\native\step_audio_runtime\scripts\build_runtime.ps1 -Version 2026.05.15 -SkipInstall -Upload
```

Then verify:

```powershell
gh release view sgt-runtime-bundles --repo nganlinh4/screen-goated-toolbox --json assets
```

Expected release assets:
- `sgt-step-audio-runtime-2026.05.15.zip.part001`
- `sgt-step-audio-runtime-2026.05.15.zip.part002`
