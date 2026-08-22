#!/usr/bin/env python3
"""Build the hosted benchmark matrix from the canonical model catalog."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from collections import defaultdict
from pathlib import Path


HOSTED_PROVIDERS = {"google", "gemini-live", "groq", "openrouter", "nvidia", "taalas"}
PROVIDER_WIDE_SHARDS = {"openrouter", "nvidia"}
SUITE_ORDER = ("text", "coordinate", "ocr")
DEFAULT_MIN_INTERVAL_MS = 2_500
GROQ_VISION_MIN_INTERVAL_MS = 12_000


def csv_values(value: str) -> set[str]:
    return {item.strip() for item in value.split(",") if item.strip()}


def build_shards(catalog: dict, requested_models: set[str], requested_suites: set[str]) -> list[dict]:
    unknown_suites = requested_suites.difference(SUITE_ORDER)
    if unknown_suites:
        raise ValueError(f"unknown suites: {sorted(unknown_suites)}")
    non_llm = set(catalog["non_llm_ids"])
    groups: dict[tuple[str, str], list[dict]] = defaultdict(list)
    for model in catalog["models"]:
        if not model.get("enabled") or model["id"] in non_llm:
            continue
        if model["provider"] not in HOSTED_PROVIDERS:
            continue
        if model["model_type"] not in {"Text", "Vision"}:
            continue
        profile = catalog["model_profiles"][f"{model['provider']}:{model['full_name']}"]
        if profile["search_tool_enabled_by_default"]:
            continue
        if requested_models and not {model["id"], model["full_name"]}.intersection(requested_models):
            continue
        provider = model["provider"]
        scope = provider if provider in PROVIDER_WIDE_SHARDS else model["full_name"]
        groups[(provider, scope)].append(model)

    shards = []
    for index, ((provider, scope), models) in enumerate(sorted(groups.items()), start=1):
        suites = set()
        if any(model["model_type"] == "Text" for model in models) and "text" in requested_suites:
            suites.add("text")
        if any(model["model_type"] == "Vision" for model in models):
            suites.update(requested_suites.intersection({"coordinate", "ocr"}))
        if not suites:
            continue
        identity = f"{provider}:{scope}"
        stem = re.sub(r"[^a-z0-9]+", "-", identity.lower()).strip("-")[:48]
        slug = f"{stem}-{hashlib.sha256(identity.encode()).hexdigest()[:8]}"
        model_ids = ",".join(sorted(model["id"] for model in models))
        # An unfiltered NVIDIA shard picks up newly discovered signed-feed rows too.
        benchmark_models = model_ids if provider != "nvidia" or requested_models else ""
        shards.append(
            {
                "index": index,
                "id": slug,
                "provider": provider,
                "models": benchmark_models,
                "catalog_models": model_ids,
                "suites": ",".join(suite for suite in SUITE_ORDER if suite in suites),
                # Vision requests consume substantially more tokens than text.
                # Pace the shared free-tier token budget instead of turning
                # predictable admission waits into false quality failures.
                "min_interval_ms": (
                    GROQ_VISION_MIN_INTERVAL_MS
                    if provider == "groq" and suites.intersection({"coordinate", "ocr"})
                    else DEFAULT_MIN_INTERVAL_MS
                ),
            }
        )
    return shards


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--catalog", type=Path, default=Path("catalog/model_catalog.json"))
    parser.add_argument("--models", default="")
    parser.add_argument("--suites", default="text,coordinate,ocr")
    args = parser.parse_args()
    catalog = json.loads(args.catalog.read_text(encoding="utf-8"))
    shards = build_shards(catalog, csv_values(args.models), csv_values(args.suites))
    if not shards:
        raise SystemExit("no hosted benchmark shards matched")
    print(json.dumps({"include": shards}, separators=(",", ":")))


if __name__ == "__main__":
    main()
