#!/usr/bin/env python
"""Screen Goated Toolbox Step Audio EditX sidecar.

This sidecar owns the Python/PyTorch model process and stays alive across
requests. It uses the official Step-Audio-EditX code paths, but avoids vLLM so
the runtime can run inside the Windows desktop app bundle.
"""

from __future__ import annotations

import json
import os
import sys
import traceback
import types
import wave
from contextlib import redirect_stdout
from pathlib import Path


_ENGINE = None
_MODEL_DIR = ""
_TOKENIZER_DIR = ""
MAX_SYNTH_ATTEMPTS = 2
MIN_AUDIO_SECONDS = 0.45

PROMPTS = {
    "default_en": (
        "prompts/zero_shot_en_prompt.wav",
        "His political stance was conservative, and he was particularly close to margaret thatcher.",
    ),
    "default_zh": (
        "prompts/fear_zh_female_prompt.wav",
        "我总觉得，有人在跟着我，我能听到奇怪的脚步声。",
    ),
}


def _err(message: str) -> None:
    print(f"[step-audio-sidecar] {message}", file=sys.stderr, flush=True)


def _runtime_root() -> Path:
    return Path(__file__).resolve().parents[1]


def _setup_python_path() -> None:
    root = _runtime_root()
    for rel in ("upstream", "upstream/Step-Audio-EditX"):
        path = str(root / rel)
        if path not in sys.path:
            sys.path.insert(0, path)
    if "vllm" not in sys.modules:
        # Upstream tokenizer.py imports model_loader.py, and model_loader.py
        # imports vLLM at module load time even when we only use FunASR token
        # extraction. Windows runtime uses transformers generation instead.
        stub = types.ModuleType("vllm")

        class _UnavailableVllm:
            def __init__(self, *args, **kwargs):
                raise RuntimeError("vLLM is not available in the Windows Step Audio runtime")

        stub.LLM = _UnavailableVllm
        stub.SamplingParams = _UnavailableVllm
        sys.modules["vllm"] = stub


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


def _chat_template_ids(tokenized):
    if isinstance(tokenized, dict):
        return tokenized["input_ids"]
    if isinstance(tokenized, list):
        return tokenized
    if hasattr(tokenized, "ids"):
        return tokenized.ids
    raise TypeError(f"Unsupported chat template output: {type(tokenized).__name__}")


def _min_pcm16_bytes(sample_rate: int) -> int:
    return int(sample_rate * MIN_AUDIO_SECONDS) * 2


def _patch_torchaudio_load() -> None:
    import numpy as np
    import soundfile as sf
    import torch
    import torchaudio

    if getattr(torchaudio.load, "_sgt_soundfile_patch", False):
        return

    def _load_with_soundfile(source, *args, **kwargs):
        if hasattr(source, "seek"):
            source.seek(0)
        data, sample_rate = sf.read(source, dtype="float32", always_2d=True)
        tensor = torch.from_numpy(np.asarray(data).T.copy())
        return tensor, sample_rate

    _load_with_soundfile._sgt_soundfile_patch = True
    torchaudio.load = _load_with_soundfile


class StepAudioEngine:
    def __init__(self, model_dir: str, tokenizer_dir: str) -> None:
        _setup_python_path()

        import torch
        from config.prompts import AUDIO_EDIT_CLONE_SYSTEM_PROMPT_TPL, AUDIO_EDIT_SYSTEM_PROMPT
        from src.model.step1_causal_lm import Step1CausalLMConfig, Step1ForCausalLM
        from stepvocoder.cosyvoice2.cli.cosyvoice import CosyVoice
        from tokenizer import StepAudioTokenizer
        from transformers import AutoTokenizer

        if not torch.cuda.is_available():
            raise RuntimeError("Step Audio EditX requires an NVIDIA CUDA GPU.")

        _patch_torchaudio_load()

        self.model_dir = model_dir
        self.tokenizer_dir = tokenizer_dir
        self.clone_prompt_template = AUDIO_EDIT_CLONE_SYSTEM_PROMPT_TPL
        self.edit_system_prompt = AUDIO_EDIT_SYSTEM_PROMPT
        self.audio_tokenizer = StepAudioTokenizer(tokenizer_dir, model_source="local")
        self.tokenizer = AutoTokenizer.from_pretrained(model_dir, trust_remote_code=True)
        model_config = Step1CausalLMConfig.from_pretrained(model_dir)
        self.model = Step1ForCausalLM.from_pretrained(
            model_dir,
            config=model_config,
            torch_dtype=torch.bfloat16,
            device_map="cuda",
        )
        self.model.eval()
        self.cosy_model = CosyVoice(
            os.path.join(model_dir, "CosyVoice-300M-25Hz"),
            dtype=torch.bfloat16,
            enable_cuda_graph=False,
        )

    def _load_prompt_audio(self, audio_path: str):
        import numpy as np
        import soundfile as sf
        import torch

        prompt_audio, prompt_wav_sr = sf.read(audio_path, dtype="float32", always_2d=True)
        prompt_wav = torch.from_numpy(np.asarray(prompt_audio).T)
        if prompt_wav.shape[0] > 1:
            prompt_wav = prompt_wav.mean(dim=0, keepdim=True)
        norm = torch.max(torch.abs(prompt_wav), dim=1, keepdim=True)[0]
        if norm > 0.6:
            prompt_wav = prompt_wav / norm * 0.6
        speech_feat, _speech_feat_len = self.cosy_model.frontend.extract_speech_feat(
            prompt_wav, prompt_wav_sr
        )
        speech_embedding = self.cosy_model.frontend.extract_spk_embedding(
            prompt_wav, prompt_wav_sr
        )
        vq0206_codes, vq02_codes_ori, vq06_codes_ori = self.audio_tokenizer.wav2token(
            prompt_wav, prompt_wav_sr
        )
        return prompt_wav, speech_feat, speech_embedding, vq0206_codes, vq02_codes_ori, vq06_codes_ori

    def _generate_audio_tokens(self, token_ids, max_new_tokens):
        import torch
        from transformers import LogitsProcessor, LogitsProcessorList

        class _AudioTokenOnlyProcessor(LogitsProcessor):
            def __call__(self, _input_ids, scores):
                mask = torch.full_like(scores, float("-inf"))
                mask[:, 65536:67584] = scores[:, 65536:67584]
                mask[:, 3] = scores[:, 3]
                return mask

        input_ids = torch.tensor([token_ids], dtype=torch.long, device=self.model.device)
        with torch.inference_mode():
            generated = self.model.generate(
                input_ids=input_ids,
                max_new_tokens=max_new_tokens,
                do_sample=True,
                temperature=0.7,
                eos_token_id=3,
                pad_token_id=0,
                logits_processor=LogitsProcessorList([_AudioTokenOnlyProcessor()]),
            )[0, input_ids.shape[1] :]
        if generated.numel() > 0 and int(generated[-1].item()) == 3:
            generated = generated[:-1]
        generated = generated.cpu()
        audio_token_mask = (generated >= 65536) & (generated < 67584)
        invalid_token_count = int((~audio_token_mask).sum().item())
        if invalid_token_count:
            print(
                f"Step Audio dropped {invalid_token_count} non-audio generated tokens",
                file=sys.stderr,
            )
        generated = generated[audio_token_mask]
        if generated.numel() == 0:
            raise RuntimeError("Step Audio generated no audio tokens")
        return generated

    def _decode_audio_tokens(self, generated, vq0206_codes, speech_feat, speech_embedding):
        import torch

        vq0206_codes_vocoder = torch.tensor([vq0206_codes], dtype=torch.long) - 65536
        return self.cosy_model.token2wav_nonstream(
            generated - 65536,
            vq0206_codes_vocoder,
            speech_feat.to(torch.bfloat16),
            speech_embedding.to(torch.bfloat16),
        )

    def synthesize(
        self,
        text: str,
        voice: str,
        prompt_audio_path: str | None,
        prompt_text: str | None,
    ):
        voice_key = voice if voice in PROMPTS else "default_en"
        if not prompt_audio_path:
            rel_audio, default_text = PROMPTS[voice_key]
            prompt_audio_path = str(_runtime_root() / rel_audio)
            # The prompt text is the exact transcript of the reference audio,
            # not a style instruction. The bundled prompt audio must stay paired
            # with its bundled transcript or the audio LLM can drift/hallucinate.
            prompt_text = default_text
        prompt_text = prompt_text or ""

        _prompt_wav, speech_feat, speech_embedding, vq0206_codes, vq02_codes_ori, vq06_codes_ori = (
            self._load_prompt_audio(prompt_audio_path)
        )
        prompt_wav_tokens = self.audio_tokenizer.merge_vq0206_to_token_str(
            vq02_codes_ori, vq06_codes_ori
        )
        system_prompt = self.clone_prompt_template.format(
            speaker="debug",
            prompt_text=prompt_text,
            prompt_wav_tokens=prompt_wav_tokens,
        )
        messages = [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": text},
        ]
        tokenized = self.tokenizer.apply_chat_template(
            messages, tokenize=True, add_generation_prompt=True
        )
        token_ids = _chat_template_ids(tokenized)
        # The Windows sidecar uses the upstream PyTorch model without vLLM's
        # paged KV cache, so keep per-request generation bounded for desktop TTS.
        max_new_tokens = max(96, min(384, 8192 - len(token_ids), len(text) * 6 + 96))
        generated = self._generate_audio_tokens(token_ids, max_new_tokens)
        audio = self._decode_audio_tokens(generated, vq0206_codes, speech_feat, speech_embedding)
        return audio, 24000

    def edit(
        self,
        source_audio_path: str,
        source_text: str,
        edit_type: str,
        edit_info: str | None,
        target_text: str | None,
    ):
        _prompt_wav, speech_feat, speech_embedding, vq0206_codes, vq02_codes_ori, vq06_codes_ori = (
            self._load_prompt_audio(source_audio_path)
        )
        audio_tokens = self.audio_tokenizer.merge_vq0206_to_token_str(vq02_codes_ori, vq06_codes_ori)
        instruction = self._build_edit_instruction(source_text, edit_type, edit_info, target_text)
        messages = [
            {"role": "system", "content": self.edit_system_prompt},
            {"role": "user", "content": f"{instruction}\n{audio_tokens}\n"},
        ]
        tokenized = self.tokenizer.apply_chat_template(
            messages, tokenize=True, add_generation_prompt=True
        )
        token_ids = _chat_template_ids(tokenized)
        max_new_tokens = max(96, min(512, 8192 - len(token_ids)))
        generated = self._generate_audio_tokens(token_ids, max_new_tokens)
        audio = self._decode_audio_tokens(generated, vq0206_codes, speech_feat, speech_embedding)
        return audio, 24000

    def _build_edit_instruction(self, source_text: str, edit_type: str, edit_info: str | None, target_text: str | None) -> str:
        source_text = source_text.strip()
        edit_info = (edit_info or "").strip()
        if edit_type in {"emotion", "speed"}:
            if edit_info == "remove":
                return f"Remove any emotion in the following audio and the reference text is: {source_text}\n"
            return f"Make the following audio more {edit_info}. The text corresponding to the audio is: {source_text}\n"
        if edit_type == "style":
            if edit_info == "remove":
                return f"Remove any speaking styles in the following audio and the reference text is: {source_text}\n"
            return f"Make the following audio more {edit_info} style. The text corresponding to the audio is: {source_text}\n"
        if edit_type == "denoise":
            return "Remove any noise from the given audio while preserving the voice content clearly. Ensure that the speech quality remains intact with minimal distortion, and eliminate all noise from the audio.\n"
        if edit_type == "vad":
            return "Remove any silent portions from the given audio while preserving the voice content clearly. Ensure that the speech quality remains intact with minimal distortion, and eliminate all silence from the audio.\n"
        if edit_type == "paralinguistic":
            return f"Add some non-verbal sounds to make the audio more natural, the new text is : {target_text or source_text}\n  The text corresponding to the audio is: {source_text}\n"
        raise RuntimeError(f"Unsupported Step Audio edit type: {edit_type}")


def _load_engine(model_dir: str, tokenizer_dir: str) -> StepAudioEngine:
    global _ENGINE, _MODEL_DIR, _TOKENIZER_DIR
    if _ENGINE is not None and _MODEL_DIR == model_dir and _TOKENIZER_DIR == tokenizer_dir:
        return _ENGINE
    _ENGINE = StepAudioEngine(model_dir, tokenizer_dir)
    _MODEL_DIR = model_dir
    _TOKENIZER_DIR = tokenizer_dir
    return _ENGINE


def _synthesize(req: dict) -> dict:
    model_dir = req["stepModelDir"]
    tokenizer_dir = req["tokenizerDir"]
    output_path = req["outputWavPath"]
    operation = req.get("operation") or "clone"

    if not os.path.isdir(model_dir):
        raise FileNotFoundError(f"Step Audio model directory not found: {model_dir}")
    if not os.path.isdir(tokenizer_dir):
        raise FileNotFoundError(f"Step Audio tokenizer directory not found: {tokenizer_dir}")

    engine = _load_engine(model_dir, tokenizer_dir)
    last_pcm16 = b""
    last_sample_rate = 24000
    for attempt in range(1, MAX_SYNTH_ATTEMPTS + 1):
        if operation == "edit":
            audio, sample_rate = engine.edit(
                req["sourceAudioPath"],
                req.get("sourceText") or "",
                req.get("editType") or "emotion",
                req.get("editInfo") or None,
                req.get("targetText") or None,
            )
        else:
            audio, sample_rate = engine.synthesize(
                req["text"],
                req.get("voice") or "default_en",
                req.get("promptAudioPath") or None,
                req.get("promptText") or None,
            )
        pcm16 = _to_pcm16_bytes(audio)
        last_pcm16 = pcm16
        last_sample_rate = sample_rate
        if len(pcm16) >= _min_pcm16_bytes(sample_rate):
            break
        _err(
            "generated too little audio "
            f"({len(pcm16)} bytes, attempt {attempt}/{MAX_SYNTH_ATTEMPTS})"
        )
    else:
        raise RuntimeError(
            "Step Audio generated too little audio "
            f"({len(last_pcm16)} bytes at {last_sample_rate} Hz)"
        )
    pcm16 = last_pcm16
    sample_rate = last_sample_rate
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
    except Exception as exc:  # noqa: BLE001
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
