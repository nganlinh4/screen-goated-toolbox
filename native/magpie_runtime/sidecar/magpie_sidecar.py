#!/usr/bin/env python
"""Screen Goated Toolbox Magpie TTS sidecar.

Reads one JSON request from stdin, writes one JSON response to stdout, and
stores synthesized mono PCM WAV at the requested output path. Diagnostics go to
stderr so Rust can parse stdout reliably.
"""

from __future__ import annotations

import json
import os
import sys
import traceback
import wave
from contextlib import redirect_stdout
from pathlib import Path


SPEAKERS = {
    "John": 0,
    "Sofia": 1,
    "Aria": 2,
    "Jason": 3,
    "Leo": 4,
}

_MODEL = None
_MODEL_PATH = ""
_CODEC_PATH = ""


def _err(message: str) -> None:
    print(f"[magpie-sidecar] {message}", file=sys.stderr, flush=True)


def _load_model(model_path: str, codec_path: str):
    global _MODEL, _MODEL_PATH, _CODEC_PATH
    if _MODEL is not None and _MODEL_PATH == model_path and _CODEC_PATH == codec_path:
        return _MODEL

    from nemo.collections.tts.models import MagpieTTSModel  # type: ignore
    from omegaconf import open_dict  # type: ignore

    config = MagpieTTSModel.restore_from(restore_path=model_path, return_config=True)
    with open_dict(config):
        config.codecmodel_path = codec_path
    try:
        model = MagpieTTSModel.restore_from(
            restore_path=model_path,
            override_config_path=config,
            map_location="cuda",
        )
    except TypeError:
        model = MagpieTTSModel.restore_from(
            restore_path=model_path,
            override_config_path=config,
            map_location="cuda",
        )
        if hasattr(model, "cfg"):
            model.cfg.codecmodel_path = codec_path
    if hasattr(model, "eval"):
        model.eval()
    _MODEL = model
    _MODEL_PATH = model_path
    _CODEC_PATH = codec_path
    return model


def _to_pcm16_bytes(audio) -> bytes:
    import numpy as np
    import torch

    if isinstance(audio, tuple):
        audio = audio[0]
    if isinstance(audio, torch.Tensor):
        audio = audio.detach().float().cpu().numpy()
    arr = np.asarray(audio, dtype=np.float32).reshape(-1)
    arr = np.clip(arr, -1.0, 1.0)
    return (arr * 32767.0).astype("<i2").tobytes()


def _write_wav(path: str, pcm16: bytes, sample_rate: int) -> None:
    output = Path(path)
    output.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(output), "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(sample_rate)
        wf.writeframes(pcm16)


def _synthesize(req: dict) -> dict:
    model_path = req["magpieModelPath"]
    codec_path = req["codecModelPath"]
    output_path = req["outputWavPath"]
    text = req["text"]
    language = req.get("language") or "en"
    voice = req.get("voice") or "Sofia"
    speaker_index = SPEAKERS.get(voice, SPEAKERS["Sofia"])

    if not os.path.isfile(model_path):
        raise FileNotFoundError(f"Magpie model not found: {model_path}")
    if not os.path.isfile(codec_path):
        raise FileNotFoundError(f"NanoCodec model not found: {codec_path}")

    model = _load_model(model_path, codec_path)

    if hasattr(model, "do_tts"):
        audio = model.do_tts(
            transcript=text,
            language=language,
            speaker_index=speaker_index,
            apply_TN=False,
        )
    elif hasattr(model, "synthesize"):
        audio = model.synthesize(text, language=language, speaker_index=speaker_index)
    else:
        raise RuntimeError("Installed NeMo Magpie model has no known TTS method")

    sample_rate = int(
        getattr(model, "output_sample_rate", None)
        or getattr(model, "sample_rate", None)
        or 22050
    )
    pcm16 = _to_pcm16_bytes(audio)
    if not pcm16:
        raise RuntimeError("Magpie generated empty audio")
    _write_wav(output_path, pcm16, sample_rate)
    return {
        "ok": True,
        "id": req.get("id") or "",
        "sampleRate": sample_rate,
        "outputWavPath": output_path,
    }


def _handle_request(raw: str) -> dict:
    try:
        req = json.loads(raw)
        with redirect_stdout(sys.stderr):
            return _synthesize(req)
    except Exception as exc:  # noqa: BLE001 - sidecar must serialize all errors
        _err(traceback.format_exc())
        return {
            "ok": False,
            "id": req.get("id", "") if isinstance(locals().get("req"), dict) else "",
            "error": str(exc),
        }


def main() -> int:
    for line in sys.stdin:
        raw = line.strip()
        if not raw:
            continue
        response = _handle_request(raw)
        print(json.dumps(response, ensure_ascii=False), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
