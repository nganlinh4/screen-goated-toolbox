# Step Audio EditX release checks

Run these checks before uploading runtime chunks. They exercise the installed
AppData runtime and model, not only the build directory.

## 1. Sidecar smoke test

```powershell
.\native\step_audio_runtime\scripts\smoke_runtime.ps1
```

Pass conditions:

- the JSON response has `"ok": true`;
- the output WAV exists; and
- the WAV contains at least 0.45 seconds of audio.

## 2. Rust integration tests

```powershell
$env:SGT_STEP_AUDIO_E2E = "1"
cargo test --target x86_64-pc-windows-gnu api::tts::worker::worker_step_audio::tests::synthesize_step_audio_e2e_when_enabled -- --nocapture

$env:SGT_STEP_AUDIO_MANAGER_E2E = "1"
cargo test --target x86_64-pc-windows-gnu api::tts::manager::tests::synthesize_to_wav_with_step_audio_profile_e2e_when_enabled -- --nocapture
```

The worker must return at least 0.5 seconds of 24 kHz audio. The manager test
must produce a valid WAV through the same collection path used by TTS
Playground and Screen Recorder narration.

To verify audible playback reaches the Windows render device, run the gated
loopback test on a machine with a usable default output device:

```powershell
$env:SGT_STEP_AUDIO_LOOPBACK_E2E = "1"
cargo test --target x86_64-pc-windows-gnu api::tts::manager::tests::step_audio_playback_loopback_e2e_when_enabled -- --nocapture
```

Clear the three environment variables after testing so normal test runs do not
unexpectedly invoke the large local model.

```powershell
Remove-Item Env:SGT_STEP_AUDIO_E2E -ErrorAction SilentlyContinue
Remove-Item Env:SGT_STEP_AUDIO_MANAGER_E2E -ErrorAction SilentlyContinue
Remove-Item Env:SGT_STEP_AUDIO_LOOPBACK_E2E -ErrorAction SilentlyContinue
```

## 3. Desktop checks

```powershell
.\scripts\run_desktop_dev.ps1
```

Verify:

- Downloaded Tools shows separate Step Audio model/tokenizer and runtime rows.
- Read Aloud produces audible speech and reports tiny, silent, or empty output
  as failure.
- Screen Recorder narration exposes Step Audio voices and creates audible
  clips.
- Clone and audio-edit requests preserve the intended reference and text.

## 4. Publish checks

Build with an explicit version, test the generated manifest against every
chunk, then upload:

```powershell
$Version = "YYYY.MM.DD"
.\native\step_audio_runtime\scripts\build_runtime.ps1 -Version $Version -SkipInstall -Upload
gh release view sgt-runtime-bundles --repo nganlinh4/screen-goated-toolbox --json assets
```

Confirm the release contains every chunk named by
`dist/sgt_step_audio_runtime.manifest.json`, with matching sizes and SHA-256
values. Commit and push that manifest only after the uploaded assets match it.
