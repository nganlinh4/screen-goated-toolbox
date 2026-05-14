#!/usr/bin/env python3
"""
sgt_tts_runtime_py — Python inference dispatch for offline open-weights TTS.

This script is invoked by `sgt_tts_runtime.dll` as a short-lived subprocess
per synthesis request. The DLL writes a JSON request to stdin, this script
runs the matching model's reference inference, and writes a WAV file to
stdout. stderr carries diagnostics.

Request shape (single line of JSON on stdin):
    {"model": "step_audio" | "voxtral",
     "model_dir": "<path to downloaded weights>",
     "text": "<utf-8 text>",
     "voice": "<voice id or empty>",
     "lang": "<bcp47 or empty>",
     "speed": 1.0}

Response: a complete WAV file (16-bit PCM mono) on stdout.

Each model's implementation is gated on the user having installed the
matching upstream package (`nemo_toolkit` / etc.).
The script imports the package lazily so unrelated models never load each
other's dependencies.
"""

import json
import sys
import io
import wave


def emit_error(message: str, code: int = 1) -> "NoReturn":  # type: ignore
    sys.stderr.write(f"[sgt_tts_runtime_py] {message}\n")
    sys.stderr.flush()
    sys.exit(code)


def write_wav(pcm16_bytes: bytes, sample_rate: int) -> None:
    """Write a mono 16-bit PCM WAV file to stdout."""
    buf = io.BytesIO()
    with wave.open(buf, "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(sample_rate)
        wf.writeframes(pcm16_bytes)
    # On Windows, stdout defaults to text mode; switch to binary explicitly.
    if hasattr(sys.stdout, "buffer"):
        sys.stdout.buffer.write(buf.getvalue())
        sys.stdout.buffer.flush()
    else:
        sys.stdout.write(buf.getvalue())
        sys.stdout.flush()


def f32_to_pcm16(samples) -> bytes:
    """Convert a numpy float32 array to little-endian PCM16 bytes."""
    import numpy as np

    arr = np.asarray(samples, dtype=np.float32)
    arr = np.clip(arr, -1.0, 1.0)
    pcm = (arr * 32767.0).astype("<i2")
    return pcm.tobytes()


# ---------------------------------------------------------------------------
# Per-model implementations
# ---------------------------------------------------------------------------
#
# Each function below loads the model's reference Python code and runs one
# utterance. The Python package install (`pip install …`) is the user's job
# and is documented in `native/<model>_runtime/README.md`. If the package is
# missing, the script exits with a clear, actionable error.


def synth_step_audio(req: dict) -> None:
    """Step Audio EditX — clone stepfun-ai/Step-Audio-EditX and add to PYTHONPATH."""
    try:
        from step_audio_editx.inference import EditXInference  # type: ignore
    except ImportError as e:
        emit_error(
            "Step Audio EditX reference package not installed. "
            "Clone https://github.com/stepfun-ai/Step-Audio-EditX and add it to PYTHONPATH, "
            f"then re-run. Original error: {e}",
            code=2,
        )

    model_dir = req["model_dir"]
    text = req["text"]
    inf = EditXInference(model_dir=model_dir)  # type: ignore[call-arg]
    audio, sr = inf.synthesize(text)
    write_wav(f32_to_pcm16(audio), sr)


def synth_voxtral(req: dict) -> None:
    """Mistral Voxtral 4B TTS — `pip install mistral-common`."""
    try:
        from mistral_inference.transformer import Transformer  # type: ignore
        from mistral_common.audio import wav_from_pcm  # type: ignore  # noqa: F401
    except ImportError as e:
        emit_error(
            "Mistral inference packages not installed. "
            "Install with: pip install mistral_inference mistral_common "
            f"Original error: {e}",
            code=2,
        )

    model_dir = req["model_dir"]
    text = req["text"]
    # The Voxtral reference inference API is in flux; follow the model card
    # on https://huggingface.co/mistralai/Voxtral-4B-TTS-2603 if this call
    # signature drifts.
    model = Transformer.from_folder(model_dir)  # type: ignore[attr-defined]
    audio, sr = model.tts(text)  # type: ignore[call-arg]
    write_wav(f32_to_pcm16(audio), sr)


DISPATCH = {
    "step_audio": synth_step_audio,
    "voxtral": synth_voxtral,
}


def main() -> None:
    raw = sys.stdin.read()
    try:
        req = json.loads(raw)
    except json.JSONDecodeError as e:
        emit_error(f"Bad JSON request: {e}", code=4)

    model = req.get("model")
    fn = DISPATCH.get(model)
    if fn is None:
        emit_error(f"Unknown model id: {model!r}", code=5)
    fn(req)


if __name__ == "__main__":
    main()
