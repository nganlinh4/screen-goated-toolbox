#!/usr/bin/env python3
"""List current zero-price OpenRouter models from the official models API."""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.request
from decimal import Decimal, InvalidOperation
from typing import Any


API_URL = "https://openrouter.ai/api/v1/models"


def zero_price(value: Any) -> bool:
    if value is None or isinstance(value, bool):
        return False
    try:
        return Decimal(str(value)) == 0
    except InvalidOperation:
        return False


def is_free(model: dict[str, Any]) -> bool:
    pricing = model.get("pricing")
    return isinstance(pricing, dict) and zero_price(pricing.get("prompt")) and zero_price(
        pricing.get("completion")
    )


def modalities(model: dict[str, Any]) -> str:
    architecture = model.get("architecture")
    if not isinstance(architecture, dict):
        return "unknown"
    values = architecture.get("input_modalities")
    if not isinstance(values, list):
        return "unknown"
    return ",".join(str(value) for value in values)


def fetch_models(api_key: str | None) -> list[dict[str, Any]]:
    headers = {"Accept": "application/json", "User-Agent": "SGT-model-catalog-audit"}
    if api_key:
        headers["Authorization"] = f"Bearer {api_key}"
    request = urllib.request.Request(API_URL, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            payload = json.load(response)
    except (urllib.error.URLError, json.JSONDecodeError) as error:
        raise RuntimeError(f"OpenRouter model inventory failed: {error}") from error
    data = payload.get("data") if isinstance(payload, dict) else None
    if not isinstance(data, list):
        raise RuntimeError("OpenRouter response has no model data array")
    return [model for model in data if isinstance(model, dict)]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="emit normalized JSON")
    parser.add_argument(
        "--vision-only", action="store_true", help="keep models accepting image input"
    )
    args = parser.parse_args()

    models = [model for model in fetch_models(os.getenv("OPENROUTER_API_KEY")) if is_free(model)]
    if args.vision_only:
        models = [model for model in models if "image" in modalities(model).split(",")]
    models.sort(key=lambda model: str(model.get("id", "")))

    normalized = [
        {
            "id": str(model.get("id", "")),
            "name": str(model.get("name", "")),
            "input_modalities": modalities(model).split(","),
            "context_length": model.get("context_length"),
            "supported_parameters": model.get("supported_parameters", []),
        }
        for model in models
    ]
    if args.json:
        json.dump(normalized, sys.stdout, ensure_ascii=False, indent=2)
        print()
    else:
        print("id\tmodalities\tcontext_length")
        for model in normalized:
            print(
                f"{model['id']}\t{','.join(model['input_modalities'])}"
                f"\t{model['context_length']}"
            )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
