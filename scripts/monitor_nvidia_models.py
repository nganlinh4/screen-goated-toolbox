#!/usr/bin/env python3
"""Probe every NVIDIA NIM model for the published availability feed.

The feed carries facts a client cannot cheaply discover for itself: whether an
endpoint answers, which general modality it accepts, and which reasoning control
silences its thinking. Latency is reported too, but only as a coarse ranking hint
-- the runner measures from one datacenter, while the client sits on the user's
own network and already records real per-call latency.

Two properties matter more than the numbers.

Reasoning control is per model, not per provider. Applying one control across a
provider fabricates failures: it rejected five working endpoints in testing, and
sending `reasoning_effort` to an endpoint that does not accept it turned a healthy
model into HTTP 500. The working control is therefore discovered per model, cached
in the history file, and re-checked only when a model stops answering.

Health is operational and hysteretic. A preset-specific answer can be useful
diagnostic evidence, but never qualifies or disqualifies a model for an entire
Text-to-Text or Image-to-Text catalog. Durable quality belongs to the catalog's
broad benchmark and review process. Three NVIDIA endpoints changed state within one day, so a
single run must never promote or demote anything; a model becomes eligible only
after several consecutive healthy runs and loses eligibility only after several
consecutive failures.
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path

# A failure reason quotes the model's own text, which is routinely not ASCII: a
# rejected reply may carry Vietnamese or Korean. On a console with a legacy code
# page that kills the run mid-probe and loses the streak counters, so the stream
# is made explicitly UTF-8 rather than left to the environment.
for _stream in (sys.stdout, sys.stderr):
    if hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8", errors="replace")

BASE_URL = "https://integrate.api.nvidia.com/v1"
# Twice the usable-latency ceiling leaves room for transport jitter and one
# silence retry without spending ninety seconds proving an unusable call slow.
REQUEST_TIMEOUT_SECONDS = "12"
SAMPLES_PER_MODEL = 3
CALL_SPACING_SECONDS = 1.6  # stays under the 40 requests-per-minute allowance

RUNS_KEPT = 24  # two days at a two-hourly cadence
RUNS_TO_PROMOTE = 3
RUNS_TO_DEMOTE = 2
AVAILABILITY_GATE_VERSION = 1
MAX_ACCEPTABLE_P50_MS = 6_000

TEXT_PROBES = (
    "Reply with exactly READY.",
    "Summarize in one sentence: A background task finished successfully and its output is ready.",
    "Return the two labels in their original order, one per line: alpha-17, beta-42.",
)
TEXT_PROMPT = TEXT_PROBES[0]

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

DEDICATED_TRANSLATION_TOKENS = {"translate", "translation", "translator"}


def published_modality(model: str, vision: object) -> str | None:
    """Generic capability advertised to clients, or None when it is dedicated."""
    tokens = set("".join(c if c.isalnum() else " " for c in model.lower()).split())
    if tokens & DEDICATED_TRANSLATION_TOKENS:
        return None
    if isinstance(vision, dict) and vision.get("passed"):
        return "vision"
    return "text"


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


def build_case_body(model: str, control: str, prompt: str) -> dict:
    body = build_body(model, control)
    body["messages"] = [m for m in body["messages"] if m["role"] != "user"]
    body["messages"].append({"role": "user", "content": prompt})
    return body


def measure(key: str, model: str, control: str) -> dict:
    """Measures whether the endpoint can serve ordinary text requests.

    An empty reply is retried once before it is recorded. Not answering is an
    availability event, and a provider drops one occasionally. Reply content is
    deliberately not judged here: these probes cover the whole text catalog, so
    no single task's expected answer is a valid universal quality gate.
    """
    latencies: list[float] = []
    failures: list[str] = []
    for prompt in TEXT_PROBES:
        body = build_case_body(model, control, prompt)
        status, payload, elapsed = post(key, body)
        time.sleep(CALL_SPACING_SECONDS)
        content, _ = answer_of(payload)
        if content is None:
            status, payload, elapsed = post(key, body)
            time.sleep(CALL_SPACING_SECONDS)
            content, _ = answer_of(payload)
        if content is None:
            failures.append(f"no reply ({status})")
            continue
        latencies.append(elapsed)
    p50 = int(statistics.median(latencies) * 1000) if latencies else None
    sample = {
        "gate": AVAILABILITY_GATE_VERSION,
        "answered": len(latencies),
        "attempts": len(TEXT_PROBES),
        "p50_ms": p50,
        "p95_ms": int(max(latencies) * 1000) if latencies else None,
    }
    passed, reason = operational_verdict(sample)
    sample["passed"] = passed
    sample["reason"] = reason or (failures[0] if failures else "")
    return sample


def measure_vision(key: str, model: str, control: str) -> dict | None:
    """Checks whether a model accepts image input and returns a useful payload.

    Vision capability is discovered rather than declared: the model list does not
    say which endpoints accept images, and several that look multimodal reject one.
    The content is not scored as OCR, localization, description, or any other
    preset because this capability feeds the whole Image-to-Text catalog.
    """
    path = Path("tests/catalog-benchmark/images/coordinate/08-filter-chip.png")
    if not path.exists():
        return None
    encoded = base64.b64encode(path.read_bytes()).decode()
    body = build_body(model, control)
    body["messages"] = [m for m in body["messages"] if m["role"] != "user"]
    body["messages"].append({
        "role": "user",
        "content": [
            {"type": "text", "text": "Describe the visible interface briefly."},
            {"type": "image_url", "image_url": {"url": f"data:image/png;base64,{encoded}"}},
        ],
    })
    body["max_tokens"] = 300
    status, payload, elapsed = post(key, body)
    time.sleep(CALL_SPACING_SECONDS)
    content, _ = answer_of(payload)
    if content is None:
        return None
    return {
        "gate": AVAILABILITY_GATE_VERSION,
        "answered": 1,
        "attempts": 1,
        "passed": True,
        "reason": "",
        "p50_ms": int(elapsed * 1000),
        "p95_ms": int(elapsed * 1000),
    }


def operational_verdict(sample: dict) -> tuple[bool, str]:
    """Catalog-wide endpoint health, independent of any preset's semantics."""
    answered = int(sample.get("answered", 0))
    attempts = int(sample.get("attempts", 0))
    p50_ms = sample.get("p50_ms")
    if attempts <= 0 or answered < max(1, attempts - 1):
        return False, f"insufficient replies ({answered}/{attempts})"
    if p50_ms is None:
        return False, "no latency"
    if p50_ms > MAX_ACCEPTABLE_P50_MS:
        return False, f"too slow ({p50_ms}ms)"
    return True, ""


def healthy(sample: dict) -> bool:
    """Whether recorded transport evidence satisfies the availability contract."""
    return operational_verdict(sample)[0]


def updated_eligibility(known: dict, sample: dict) -> tuple[int, int, bool]:
    """Streak state after one sample, scoped only to operational semantics.

    On migration, recent transport evidence is re-evaluated under this contract.
    That restores endpoints which were operationally sound but failed an old
    preset-specific content check, without restoring silent or slow endpoints.
    """
    if known.get("eligibility_gate") != AVAILABILITY_GATE_VERSION:
        statuses = [healthy(item) for item in known.get("recent", [])]
        streak_ok = 0
        for status in reversed(statuses):
            if not status:
                break
            streak_ok += 1
        streak_bad = 0
        for status in reversed(statuses):
            if status:
                break
            streak_bad += 1
        eligible = streak_ok >= RUNS_TO_PROMOTE and streak_bad < RUNS_TO_DEMOTE
    else:
        streak_ok = known.get("healthy_streak", 0)
        streak_bad = known.get("failing_streak", 0)
        eligible = known.get("eligible", False)
    if healthy(sample):
        streak_ok, streak_bad = streak_ok + 1, 0
    else:
        streak_ok, streak_bad = 0, streak_bad + 1
    if streak_ok >= RUNS_TO_PROMOTE:
        eligible = True
    if streak_bad >= RUNS_TO_DEMOTE:
        eligible = False
    return streak_ok, streak_bad, eligible


def run(history: dict, key: str, limit: int | None, only: list[str] | None = None) -> dict:
    models = only or list_models(key)
    if limit:
        models = models[:limit]
    previous = history.get("models", {})
    # A focused probe updates only named endpoints. Keeping the rest is essential:
    # an emergency check must not make every unselected model disappear.
    results: dict[str, dict] = dict(previous) if only is not None else {}
    for model in models:
        known = previous.get(model, {})
        control = known.get("control")
        if not control:
            control, _ = discover_control(key, model)
        sample = measure(key, model, control) if control else {
            "gate": AVAILABILITY_GATE_VERSION,
            "answered": 0, "attempts": len(TEXT_PROBES), "passed": False,
            "reason": "no working reasoning control", "p50_ms": None, "p95_ms": None,
        }
        if control and sample["answered"] == 0:
            # It may simply need a different control now; re-discover once.
            control, _ = discover_control(key, model)
            if control:
                sample = measure(key, model, control)
        # Whether an endpoint accepts images is a property of the endpoint and
        # stays cached. Preset-specific image output quality is catalog evidence,
        # not live availability evidence.
        vision = known.get("vision")
        stale = isinstance(vision, dict) and vision.get("gate") != AVAILABILITY_GATE_VERSION
        if control and healthy(sample) and (vision is None or stale):
            measured = measure_vision(key, model, control)
            # None means it refused an image, which is capability, not quality.
            vision = measured if measured is not None else False

        streak_ok, streak_bad, eligible = updated_eligibility(known, sample)
        recent = ([*known.get("recent", []), sample])[-RUNS_KEPT:]
        results[model] = {
            "control": control,
            "vision": vision,
            "eligibility_gate": AVAILABILITY_GATE_VERSION,
            "healthy_streak": streak_ok,
            "failing_streak": streak_bad,
            "eligible": eligible,
            "recent": recent,
        }
        modality = published_modality(model, vision) or "dedicated"
        note = "" if sample.get("passed") else f"  [{sample.get('reason')}]"
        print(f"{model:<52}{str(control):<16}{modality:<7}"
              f"{sample['answered']}/{sample['attempts']} "
              f"p50={sample['p50_ms']} eligible={eligible}{note}",
              flush=True)
    return {"models": results}


def published(history: dict, generated_at: str) -> dict:
    """The client-facing view: operationally healthy general models, with what a client
    needs to use one without shipping a new build.

    `modality` and `control` are carried because the client cannot infer them and
    must not guess: the wrong modality routes an image to a text endpoint, and the
    wrong control turns a healthy model into an error or a bill for reasoning
    nobody reads.
    """
    entries = []
    for model, state in history["models"].items():
        if not state.get("eligible"):
            continue
        # Eligibility is deliberately sticky, so that a single bad run does not
        # evict a good model. Publication is not: a model that failed its most
        # recent run is not offered, even while it keeps its eligibility. Without
        # this, a tightened gate publishes models at zero percent success until
        # two consecutive failures demote them.
        if not (state["recent"] and healthy(state["recent"][-1])):
            continue
        comparable = list(state["recent"])
        recent = [s for s in comparable if s.get("p50_ms") is not None]
        if not recent:
            continue
        passes = sum(1 for s in comparable if healthy(s))
        vision = state.get("vision")
        modality = published_modality(model, vision)
        if modality is None:
            continue
        entries.append({
            "id": model,
            "control": state["control"],
            "modality": modality,
            "p50_ms": int(statistics.median(s["p50_ms"] for s in recent)),
            "success_rate": round(passes / len(comparable), 3),
            "runs": len(comparable),
        })
    entries.sort(key=lambda e: e["p50_ms"])
    return {
        "schemaVersion": 3,
        "controlVersion": 1,
        "availabilityGateVersion": AVAILABILITY_GATE_VERSION,
        "provider": "nvidia",
        "generatedAt": generated_at,
        "note": ("Eligibility measures endpoint availability, modality, and latency only. "
                 "Durable quality comes from the catalog benchmark; clients should refine "
                 "latency order using their own observations."),
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
