# Prompt DJ parity

## Canonical Source

- Windows host: `src/overlay/prompt_dj/`.
- Shared interface: `promptdj-midi/`, delivered to Windows and Android.
- Playback and recording graph: `promptdj-midi/utils/LiveMusicHelper.ts` and `promptdj-midi/index.tsx`.

## Behavior Contract

- The volume slider controls only Prompt DJ speaker playback and recording-preview playback.
- A slider change made before audio initialization applies when playback starts; it remains effective through stop and restart.
- The recording and analyser tap precedes the playback-volume gain, so changing the slider does not change recorded samples.
- Other application audio sessions and operating-system volume settings remain untouched.
- Windows and Android use the same frontend and audio graph.

## Failure And Recovery

- Stopping or restarting playback does not reset the current slider setting.
- Failed audio initialization does not change operating-system or other application audio volume.

## Fixtures

- `parity-fixtures/prompt-dj/volume.json` describes volume and recording invariants.
- Shared frontend tests exercise the playback graph and slider state without a provider or audio device.

## Deviations

- Real WebView and device audio acceptance remains required on both platforms.
