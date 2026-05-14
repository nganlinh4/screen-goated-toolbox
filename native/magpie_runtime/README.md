# Magpie Managed Runtime

NVIDIA Magpie-Multilingual 357M is a NeMo `.nemo` checkpoint, not a native
DLL. Screen Goated Toolbox runs it through a managed Python/NeMo sidecar that
is downloaded into the app data directory.

## Runtime contents

- Python 3.11 runtime
- PyTorch CUDA
- NeMo TTS
- `kaldialign`
- `sidecar/magpie_sidecar.py` packaged as `magpie-sidecar.exe`

The model files are not bundled with the runtime:

- `magpie_tts_multilingual_357m.nemo`
- `nemo-nano-codec-22khz-1.89kbps-21.5fps.nemo`

The Rust app downloads those from Hugging Face as model assets.

## Sidecar protocol

The sidecar reads one JSON request from stdin and writes one JSON response to
stdout. Audio is written to `outputWavPath`.

Request:

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

Response:

```json
{
  "ok": true,
  "sampleRate": 22050,
  "outputWavPath": "C:/.../magpie.wav"
}
```

Errors use `{"ok": false, "error": "..."}` and diagnostics go to stderr.

## Download manifest

The app fetches:

`native/magpie_runtime/dist/sgt_magpie_runtime.manifest.json`

from the project `main` branch. The manifest points to one or more zipped
runtime chunks. Each chunk must be less than the hosting provider's per-file
limit and must include size and SHA-256.

Manifest shape:

```json
{
  "version": "2026.05.14",
  "abiVersion": 1,
  "entrypoint": "magpie-sidecar/magpie-sidecar.exe",
  "installedSize": 0,
  "chunks": [
    {
      "filename": "sgt-magpie-runtime.zip.part1",
      "url": "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/magpie-runtime-2026.05.14/sgt-magpie-runtime.zip.part1",
      "sha256": "<64 hex chars>",
      "size": 123
    }
  ]
}
```

The archive must extract so the entrypoint exists at the relative path above.
