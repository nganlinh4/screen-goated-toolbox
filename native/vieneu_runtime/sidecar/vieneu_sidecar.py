import json
import os
import sys
import traceback

import numpy as np
import soundfile as sf


ENGINES = {}


def _configure_stdio():
    for stream in (sys.stdin, sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8", errors="replace")
        except Exception:
            pass


def _configure_runtime_environment():
    local_app_data = os.environ.get("LOCALAPPDATA")
    if local_app_data:
        hf_home = os.path.join(
            local_app_data, "screen-goated-toolbox", "models", "vieneu_hf"
        )
        os.environ["HF_HOME"] = hf_home
        os.environ["HF_HUB_CACHE"] = os.path.join(hf_home, "hub")
        os.environ["TRANSFORMERS_CACHE"] = os.path.join(hf_home, "transformers")
    os.environ["HF_HUB_DISABLE_SYMLINKS"] = "1"
    os.environ["HF_HUB_DISABLE_SYMLINKS_WARNING"] = "1"


def _patch_vieneu_turbo_budget():
    turbo_path = os.path.join(
        os.path.dirname(sys.executable),
        "..",
        "Lib",
        "site-packages",
        "vieneu",
        "turbo.py",
    )
    turbo_path = os.path.abspath(turbo_path)
    try:
        with open(turbo_path, "r", encoding="utf-8") as handle:
            body = handle.read()
        old = "max_new_tokens=2048,"
        new = 'max_new_tokens=int(getattr(self, "_sgt_max_new_tokens", 384)),'
        if old in body and new not in body:
            with open(turbo_path, "w", encoding="utf-8") as handle:
                handle.write(body.replace(old, new))
    except Exception as exc:
        print(f"[VieNeu sidecar] failed to patch turbo token budget: {exc}", file=sys.stderr)


_configure_stdio()
_configure_runtime_environment()
_patch_vieneu_turbo_budget()


def _clean_text(value):
    text = str(value or "").strip()
    if not text:
        return ""
    return text.encode("utf-8", "replace").decode("utf-8").replace("\ufffd", "")


def _trim_silence(wav, sample_rate):
    if wav.size == 0:
        return wav
    peak = float(np.max(np.abs(wav)))
    if peak < 1.0 / 32768.0:
        raise RuntimeError("VieNeu returned only silence")
    threshold = min(220.0 / 32768.0, max(24.0 / 32768.0, peak * 0.03))
    window = max(1, int(sample_rate * 0.02))
    pad = max(1, int(sample_rate * 0.14))
    first = None
    last = None
    for start in range(0, wav.size, window):
        chunk = wav[start : start + window]
        if chunk.size and float(np.max(np.abs(chunk))) >= threshold:
            if first is None:
                first = start
            last = min(start + chunk.size, wav.size)
    if first is None or last is None:
        raise RuntimeError(f"VieNeu returned only silence (peak={peak:.6f})")
    start = max(0, first - pad)
    end = min(wav.size, last + pad)
    if end <= start:
        print(
            f"[VieNeu sidecar] silence trim skipped: invalid range peak={peak:.6f}",
            file=sys.stderr,
        )
        return wav
    if start > 0 or end < wav.size:
        before_ms = int(wav.size * 1000 / sample_rate)
        after_ms = int((end - start) * 1000 / sample_rate)
        print(
            f"[VieNeu sidecar] trimmed silence: {before_ms}ms -> {after_ms}ms",
            file=sys.stderr,
        )
    return wav[start:end]


def _build_engine(req):
    from vieneu import Vieneu

    mode = req.get("mode") or "turbo_gpu"
    repo = req.get("backboneRepo") or "pnnbao-ump/VieNeu-TTS-v2-Turbo"
    backend = req.get("backend") or "standard"
    backbone_device = req.get("backboneDevice") or "cuda"
    codec_device = req.get("codecDevice") or "cpu"
    if mode == "turbo_gpu":
        return Vieneu(
            mode="turbo_gpu",
            backbone_repo=repo,
            device=backbone_device,
            backend=backend,
        )
    if mode == "turbo":
        return Vieneu(mode="turbo", backbone_repo=repo, device=backbone_device)
    if mode == "fast":
        return Vieneu(
            mode="fast",
            backbone_repo=repo,
            backbone_device=backbone_device,
            codec_device=codec_device,
        )
    return Vieneu(
        mode="standard",
        backbone_repo=repo,
        backbone_device=backbone_device,
        codec_device=codec_device,
    )


def _engine(req):
    key = json.dumps(
        {
            "mode": req.get("mode"),
            "repo": req.get("backboneRepo"),
            "backend": req.get("backend"),
            "backboneDevice": req.get("backboneDevice"),
            "codecDevice": req.get("codecDevice"),
        },
        sort_keys=True,
    )
    if key not in ENGINES:
        ENGINES[key] = _build_engine(req)
    return ENGINES[key]


def _synthesize(req):
    tts = _engine(req)
    text = _clean_text(req.get("text"))
    if not text:
        raise ValueError("Text is empty")

    reference_audio = (req.get("referenceAudioPath") or "").strip()
    reference_text = _clean_text(req.get("referenceText"))
    mode = req.get("mode") or "turbo_gpu"
    kwargs = {
        "temperature": float(req.get("temperature") or 0.4),
        "top_k": int(req.get("topK") or 50),
    }
    if mode in ("turbo", "turbo_gpu"):
        max_new_tokens = int(req.get("maxNewTokens") or (len(text) * 2 + 24))
        tts._sgt_max_new_tokens = max(96, min(384, max_new_tokens))

    if reference_audio:
        if mode in ("turbo", "turbo_gpu"):
            voice = tts.encode_reference(reference_audio)
            wav = tts.infer(text=text, voice=voice, **kwargs)
        else:
            if not reference_text:
                raise ValueError(
                    "VieNeu standard/fast cloning needs the exact reference transcript."
                )
            wav = tts.infer(
                text=text,
                ref_audio=reference_audio,
                ref_text=reference_text,
                **kwargs,
            )
    else:
        wav = tts.infer(text=text, **kwargs)

    if wav is None:
        raise RuntimeError("VieNeu returned no audio")
    wav = np.asarray(wav, dtype=np.float32).flatten()
    if wav.size == 0:
        raise RuntimeError("VieNeu returned empty audio")

    out = req.get("outputWavPath")
    if not out:
        raise ValueError("outputWavPath is required")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    sample_rate = int(getattr(tts, "sample_rate", 24000) or 24000)
    wav = _trim_silence(wav, sample_rate)
    sf.write(out, wav, sample_rate)
    return {"ok": True, "sampleRate": sample_rate, "outputWavPath": out}


def _handle(req):
    response = {"id": req.get("id", ""), "ok": False}
    try:
        response.update(_synthesize(req))
    except Exception as exc:
        details = traceback.format_exc()
        print(details, file=sys.stderr)
        response["error"] = f"{exc}\n{details}"
    return response


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            response = _handle(req)
        except Exception as exc:
            details = traceback.format_exc()
            print(details, file=sys.stderr)
            response = {"id": "", "ok": False, "error": f"{exc}\n{details}"}
        print(json.dumps(response, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
