#!/usr/bin/env python3
"""Probe every NVIDIA NIM model and score it for the published availability feed.

The feed carries facts a client cannot cheaply discover for itself: whether an
endpoint answers at all, whether its answer is correct, and which reasoning
control silences its thinking. Latency is reported too, but only as a coarse
ranking hint -- the runner measures from one datacenter, while the client sits on
the user's own network and already records real per-call latency.

Two properties matter more than the numbers.

Reasoning control is per model, not per provider. Applying one control across a
provider fabricates failures: it rejected five working endpoints in testing, and
sending `reasoning_effort` to an endpoint that does not accept it turned a healthy
model into HTTP 500. The working control is therefore discovered per model, cached
in the history file, and re-checked only when a model stops answering.

Health is hysteretic. Three NVIDIA endpoints changed state within one day, so a
single run must never promote or demote anything; a model becomes eligible only
after several consecutive healthy runs and loses eligibility only after several
consecutive failures.
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path

BASE_URL = "https://integrate.api.nvidia.com/v1"
REQUEST_TIMEOUT_SECONDS = "45"
SAMPLES_PER_MODEL = 3
CALL_SPACING_SECONDS = 1.6  # stays under the 40 requests-per-minute allowance

RUNS_KEPT = 24  # two days at a two-hourly cadence
RUNS_TO_PROMOTE = 3
RUNS_TO_DEMOTE = 2

TEXT_PROMPT = (
    "Translate to Vietnamese, output only the translation: "
    '"Settings > Display > Night light. Turn on automatically at sunset."'
)
TEXT_EXPECTED = ("cài đặt", "màn hình", "hiển thị", "đèn", "ánh sáng", "hoàng hôn", "mặt trời lặn")
TEXT_MIN_MATCHES = 2

# Ordered by preference: an endpoint that reaches zero reasoning tokens on an
# earlier control never needs a later one.
CONTROLS = (
    ("plain", {}, []),
    ("effort-none", {"reasoning_effort": "none"}, []),
    ("no-think", {}, [{"role": "system", "content": "/no_think"}]),
    ("thinking-off", {}, [{"role": "system", "content": "detailed thinking off"}]),
    ("template-kwargs", {"chat_template_kwargs": {"thinking": False}}, []),
    ("effort-low", {"reasoning_effort": "low"}, []),
)

SKIP_SUBSTRINGS = (
    "embed", "rerank", "guard", "safety", "jailbreak", "gliner", "pii", "topic-control",
    "asr", "tts", "stt", "whisper", "parakeet", "riva-asr", "riva-tts", "ocdr",
    "flux", "stable", "sana", "consistory", "edify", "picasso", "image-edit", "nvclip",
    "maisi", "vista", "segment", "cuopt", "earth2", "fourcastnet", "molmim", "esm",
    "diffdock", "genmol", "protein", "rfdiffusion", "alphafold", "video", "audio2face",
    "retriever", "reward", "deplot", "kosmos", "fuyu", "ising",
)


def api_key() -> str:
    key = os.environ.get("NVIDIA_API_KEY", "").strip()
    if not key:
        raise SystemExit("NVIDIA_API_KEY is not set")
    return key


def post(key: str, body: dict) -> tuple[str, dict, float]:
    """Returns the HTTP status, decoded body, and elapsed seconds."""
    started = time.monotonic()
    completed = subprocess.run(
        ["curl", "-s", "-m", REQUEST_TIMEOUT_SECONDS, "-w", "\n%{http_code}",
         "-X", "POST", f"{BASE_URL}/chat/completions",
         "-H", f"Authorization: Bearer {key}",
         "-H", "content-type: application/json",
         "--data-binary", "@-"],
        input=json.dumps(body), capture_output=True, text=True,
        encoding="utf-8", errors="replace", check=False,
    )
    elapsed = time.monotonic() - started
    head, _, status = completed.stdout.rpartition("\n")
    try:
        return status.strip(), json.loads(head), elapsed
    except json.JSONDecodeError:
        return status.strip(), {}, elapsed


def list_models(key: str) -> list[str]:
    completed = subprocess.run(
        ["curl", "-s", "-m", "60", f"{BASE_URL}/models",
         "-H", f"Authorization: Bearer {key}"],
        capture_output=True, text=True, encoding="utf-8", errors="replace", check=False,
    )
    try:
        entries = json.loads(completed.stdout).get("data", [])
    except json.JSONDecodeError:
        raise SystemExit("could not read the NVIDIA model list")
    names = sorted(entry["id"] for entry in entries)
    return [n for n in names if not any(s in n.lower() for s in SKIP_SUBSTRINGS)]


def answer_of(payload: dict) -> tuple[str | None, int]:
    """Content and reasoning-character count of a completion."""
    choices = payload.get("choices") or []
    if not choices:
        return None, 0
    message = choices[0].get("message") or {}
    reasoning = message.get("reasoning_content") or message.get("reasoning") or ""
    content = (message.get("content") or "").strip()
    return (content or None), len(reasoning)


def is_correct(text: str) -> bool:
    lowered = text.lower()
    return sum(1 for token in TEXT_EXPECTED if token in lowered) >= TEXT_MIN_MATCHES


def discover_control(key: str, model: str) -> tuple[str | None, int]:
    """First control that produces an answer, preferring zero reasoning."""
    fallback: tuple[str, int] | None = None
    for label, extra, system in CONTROLS:
        body = {"model": model, "max_tokens": 400, "temperature": 0,
                "messages": [*system, {"role": "user", "content": TEXT_PROMPT}], **extra}
        status, payload, _ = post(key, body)
        time.sleep(CALL_SPACING_SECONDS)
        content, reasoning = answer_of(payload)
        if content:
            if reasoning == 0:
                return label, reasoning
            fallback = fallback or (label, reasoning)
            continue
        # A transport or server failure will not be fixed by another control.
        if status not in {"400", "422"}:
            break
    return fallback if fallback else (None, 0)


def build_body(model: str, control: str) -> dict:
    for label, extra, system in CONTROLS:
        if label == control:
            return {"model": model, "max_tokens": 400, "temperature": 0,
                    "messages": [*system, {"role": "user", "content": TEXT_PROMPT}], **extra}
    return {"model": model, "max_tokens": 400, "temperature": 0,
            "messages": [{"role": "user", "content": TEXT_PROMPT}]}


def measure(key: str, model: str, control: str) -> dict:
    latencies: list[float] = []
    correct = 0
    for _ in range(SAMPLES_PER_MODEL):
        _, payload, elapsed = post(key, build_body(model, control))
        time.sleep(CALL_SPACING_SECONDS)
        content, _ = answer_of(payload)
        if content:
            latencies.append(elapsed)
            correct += int(is_correct(content))
    attempts = SAMPLES_PER_MODEL
    return {
        "answered": len(latencies),
        "attempts": attempts,
        "correct": correct,
        "p50_ms": int(statistics.median(latencies) * 1000) if latencies else None,
        "p95_ms": int(max(latencies) * 1000) if latencies else None,
    }


def healthy(sample: dict) -> bool:
    """Every sample answered, and a majority were correct."""
    return (
        sample["answered"] == sample["attempts"]
        and sample["correct"] * 2 > sample["attempts"]
    )


def run(history: dict, key: str, limit: int | None, only: list[str] | None = None) -> dict:
    models = only or list_models(key)
    if limit:
        models = models[:limit]
    previous = history.get("models", {})
    results: dict[str, dict] = {}
    for model in models:
        known = previous.get(model, {})
        control = known.get("control")
        if not control:
            control, _ = discover_control(key, model)
        sample = measure(key, model, control) if control else {
            "answered": 0, "attempts": SAMPLES_PER_MODEL, "correct": 0,
            "p50_ms": None, "p95_ms": None,
        }
        if control and sample["answered"] == 0:
            # It may simply need a different control now; re-discover once.
            control, _ = discover_control(key, model)
            if control:
                sample = measure(key, model, control)
        streak_ok = known.get("healthy_streak", 0)
        streak_bad = known.get("failing_streak", 0)
        if healthy(sample):
            streak_ok, streak_bad = streak_ok + 1, 0
        else:
            streak_ok, streak_bad = 0, streak_bad + 1
        eligible = known.get("eligible", False)
        if streak_ok >= RUNS_TO_PROMOTE:
            eligible = True
        if streak_bad >= RUNS_TO_DEMOTE:
            eligible = False
        recent = ([*known.get("recent", []), sample])[-RUNS_KEPT:]
        results[model] = {
            "control": control,
            "healthy_streak": streak_ok,
            "failing_streak": streak_bad,
            "eligible": eligible,
            "recent": recent,
        }
        print(f"{model:<52}{str(control):<16}"
              f"{sample['answered']}/{sample['attempts']} "
              f"correct={sample['correct']} p50={sample['p50_ms']} eligible={eligible}",
              flush=True)
    return {"models": results}


def published(history: dict, generated_at: str) -> dict:
    """The client-facing view: eligible models only, with a ranking hint."""
    entries = []
    for model, state in history["models"].items():
        if not state.get("eligible"):
            continue
        recent = [s for s in state["recent"] if s["p50_ms"] is not None]
        if not recent:
            continue
        answered = sum(s["answered"] for s in state["recent"])
        attempts = sum(s["attempts"] for s in state["recent"])
        entries.append({
            "id": model,
            "control": state["control"],
            "p50_ms": int(statistics.median(s["p50_ms"] for s in recent)),
            "success_rate": round(answered / attempts, 3) if attempts else 0.0,
            "runs": len(state["recent"]),
        })
    entries.sort(key=lambda e: e["p50_ms"])
    return {
        "schemaVersion": 1,
        "provider": "nvidia",
        "generatedAt": generated_at,
        "note": ("Latency is measured from one datacenter and is a ranking hint only; "
                 "clients should order by their own observed latency."),
        "models": entries,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--history", type=Path, default=Path("monitoring/nvidia-history.json"))
    parser.add_argument("--output", type=Path, default=Path("monitoring/nvidia-availability.json"))
    parser.add_argument("--limit", type=int, default=None, help="probe only the first N models")
    parser.add_argument("--models", default=None,
                        help="comma-separated model ids to probe instead of the full list")
    parser.add_argument("--generated-at", default=None, help="timestamp for the published feed")
    args = parser.parse_args()

    history = {}
    if args.history.exists():
        history = json.loads(args.history.read_text(encoding="utf-8"))
    only = [m.strip() for m in args.models.split(",") if m.strip()] if args.models else None
    history = run(history, api_key(), args.limit, only)

    args.history.parent.mkdir(parents=True, exist_ok=True)
    args.history.write_text(json.dumps(history, ensure_ascii=False, indent=1) + "\n",
                            encoding="utf-8")
    stamp = args.generated_at or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    feed = published(history, stamp)
    args.output.write_text(json.dumps(feed, ensure_ascii=False, indent=1) + "\n",
                           encoding="utf-8")
    eligible = len(feed["models"])
    print(f"\n{eligible} eligible of {len(history['models'])} probed -> {args.output}")


if __name__ == "__main__":
    main()
