#!/usr/bin/env python3
"""Collect API-first provider discovery evidence without exposing credentials."""

from __future__ import annotations

import argparse
import hashlib
import html
import json
import os
import re
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from html.parser import HTMLParser
from pathlib import Path
from typing import Any


GEMINI_MODELS_URL = "https://generativelanguage.googleapis.com/v1beta/models"
GEMINI_MODELS_DOC = "https://ai.google.dev/api/models"
GEMINI_PRICING_URL = "https://ai.google.dev/gemini-api/docs/pricing"
GROQ_MODELS_URL = "https://api.groq.com/openai/v1/models"
GROQ_MODELS_DOC = "https://console.groq.com/docs/api-reference"
GROQ_FREE_LIMITS_URL = "https://console.groq.com/docs/rate-limits"
USER_AGENT = "SGT-catalog-discovery/1"


def read_dotenv(path: Path) -> dict[str, str]:
    if not path.is_file():
        return {}
    values: dict[str, str] = {}
    for raw_line in path.read_text(encoding="utf-8-sig").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[7:].lstrip()
        match = re.match(r"^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$", line)
        if not match:
            continue
        value = match.group(2).strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        values[match.group(1)] = value
    return values


def credential_pool(prefix: str, dotenv: dict[str, str]) -> list[str]:
    combined = dict(dotenv)
    combined.update({key: value for key, value in os.environ.items() if value})
    values: list[str] = []
    array_name = f"{prefix}S_JSON"
    if combined.get(array_name):
        parsed = json.loads(combined[array_name])
        if not isinstance(parsed, list) or not all(isinstance(item, str) for item in parsed):
            raise ValueError(f"{array_name} must be a JSON string array")
        values.extend(item.strip() for item in parsed if item.strip())
    slots: dict[int, str] = {}
    for name, value in combined.items():
        if name == prefix:
            slots[1] = value
            continue
        match = re.fullmatch(rf"{re.escape(prefix)}_([1-9][0-9]*)", name)
        if match:
            slots[int(match.group(1))] = value
    values.extend(value.strip() for _, value in sorted(slots.items()) if value.strip())
    return list(dict.fromkeys(values))


def request_json(url: str, headers: dict[str, str]) -> dict[str, Any]:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, **headers})
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def request_text(url: str) -> str:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.read().decode(response.headers.get_content_charset() or "utf-8")


def safe_error(error: Exception) -> str:
    if isinstance(error, urllib.error.HTTPError):
        return f"HTTP {error.code}"
    if isinstance(error, urllib.error.URLError):
        return f"network error: {type(error.reason).__name__}"
    return type(error).__name__


def fingerprint(ids: list[str]) -> str:
    digest = hashlib.sha256("\n".join(sorted(ids)).encode()).hexdigest()
    return f"sha256:{digest}"


class TableRows(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.rows: list[list[str]] = []
        self._row: list[str] | None = None
        self._cell: list[str] | None = None

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        del attrs
        if tag == "tr":
            self._row = []
        elif tag in {"td", "th"} and self._row is not None:
            self._cell = []

    def handle_data(self, data: str) -> None:
        if self._cell is not None:
            self._cell.append(data)

    def handle_endtag(self, tag: str) -> None:
        if tag in {"td", "th"} and self._cell is not None and self._row is not None:
            self._row.append(" ".join("".join(self._cell).split()))
            self._cell = None
        elif tag == "tr" and self._row is not None:
            if self._row:
                self.rows.append(self._row)
            self._row = None


def groq_free_rows(document: str) -> dict[str, dict[str, str]]:
    parser = TableRows()
    parser.feed(document)
    result: dict[str, dict[str, str]] = {}
    headers = ("rpm", "rpd", "tpm", "tpd", "ash", "asd")
    for row in parser.rows:
        if len(row) != 7 or row[0].upper() == "MODEL ID":
            continue
        model_id = row[0].strip()
        if not model_id or not any(character in model_id for character in "/-"):
            continue
        result.setdefault(model_id, dict(zip(headers, row[1:], strict=True)))
    return result


def clean_markup(value: str) -> str:
    return " ".join(html.unescape(re.sub(r"<[^>]+>", " ", value)).split())


def normalized_name(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", " ", value.lower()).strip()


def gemini_pricing_sections(document: str) -> list[tuple[str, str]]:
    matches = list(
        re.finditer(r"<h([2-4])[^>]*>(.*?)</h\1>", document, re.IGNORECASE | re.DOTALL)
    )
    sections: list[tuple[str, str]] = []
    for index, match in enumerate(matches):
        level = int(match.group(1))
        end = len(document)
        for later in matches[index + 1 :]:
            if int(later.group(1)) <= level:
                end = later.start()
                break
        sections.append((clean_markup(match.group(2)), clean_markup(document[match.end() : end])))
    return sections


def gemini_pricing_status(display_name: str, sections: list[tuple[str, str]]) -> dict[str, str]:
    model_name = normalized_name(display_name)
    candidates: list[tuple[int, str, str]] = []
    for heading, section in sections:
        heading_name = normalized_name(heading)
        if not heading_name or not (model_name.startswith(heading_name) or heading_name.startswith(model_name)):
            continue
        candidates.append((min(len(model_name), len(heading_name)), heading, section))
    if not candidates:
        return {"status": "not-documented", "pricing_heading": ""}
    longest = max(score for score, _, _ in candidates)
    best = [(heading, section) for score, heading, section in candidates if score == longest]
    if len(best) != 1:
        return {"status": "ambiguous", "pricing_heading": ""}
    heading, section = best[0]
    lowered = section.lower()
    if "free of charge" in lowered:
        status = "documented-free"
    elif "free tier" in lowered or "not available" in lowered:
        status = "documented-unavailable"
    else:
        status = "ambiguous"
    return {"status": status, "pricing_heading": heading}


def gemini_inventory(keys: list[str]) -> dict[str, Any]:
    observations: list[dict[str, Any]] = []
    for key in keys:
        try:
            query = urllib.parse.urlencode({"pageSize": 1000})
            payload = request_json(f"{GEMINI_MODELS_URL}?{query}", {"x-goog-api-key": key})
            models = payload.get("models", [])
            observations.append(
                {
                    "ok": True,
                    "models": models,
                    "fingerprint": fingerprint([model["name"] for model in models]),
                    "next_page": bool(payload.get("nextPageToken")),
                }
            )
        except Exception as error:  # evidence must retain provider failure without secrets
            observations.append({"ok": False, "error": safe_error(error)})
    successes = [observation for observation in observations if observation["ok"]]
    models = successes[0]["models"] if successes else []
    return {
        "endpoint": GEMINI_MODELS_URL,
        "documentation": GEMINI_MODELS_DOC,
        "credential_count": len(keys),
        "successful_credentials": len(successes),
        "inventory_variants": len({item["fingerprint"] for item in successes}),
        "inventory_fingerprints": sorted({item["fingerprint"] for item in successes}),
        "pagination_observed": any(item["next_page"] for item in successes),
        "errors": [item["error"] for item in observations if not item["ok"]],
        "models": models,
    }


def groq_inventory(keys: list[str]) -> dict[str, Any]:
    observations: list[dict[str, Any]] = []
    for key in keys:
        try:
            payload = request_json(GROQ_MODELS_URL, {"Authorization": f"Bearer {key}"})
            models = payload.get("data", [])
            observations.append(
                {
                    "ok": True,
                    "models": models,
                    "fingerprint": fingerprint([model["id"] for model in models]),
                }
            )
        except Exception as error:  # evidence must retain provider failure without secrets
            observations.append({"ok": False, "error": safe_error(error)})
    successes = [observation for observation in observations if observation["ok"]]
    models = successes[0]["models"] if successes else []
    return {
        "endpoint": GROQ_MODELS_URL,
        "documentation": GROQ_MODELS_DOC,
        "credential_count": len(keys),
        "successful_credentials": len(successes),
        "inventory_variants": len({item["fingerprint"] for item in successes}),
        "inventory_fingerprints": sorted({item["fingerprint"] for item in successes}),
        "errors": [item["error"] for item in observations if not item["ok"]],
        "models": models,
    }


def catalog_endpoints(catalog: dict[str, Any], providers: set[str]) -> set[str]:
    non_llm = set(catalog["non_llm_ids"])
    endpoints: set[str] = set()
    for model in catalog["models"]:
        if not model.get("enabled") or model["provider"] not in providers:
            continue
        if model["model_type"] not in {"Text", "Vision"} or model["id"] in non_llm:
            continue
        profile = catalog["model_profiles"][f"{model['provider']}:{model['full_name']}"]
        if not profile["search_tool_enabled_by_default"]:
            endpoints.add(model["full_name"])
    return endpoints


def build_report(
    catalog: dict[str, Any], gemini: dict[str, Any], groq: dict[str, Any],
    gemini_pricing_html: str | None, groq_limits_html: str | None,
) -> dict[str, Any]:
    pricing_sections = gemini_pricing_sections(gemini_pricing_html or "")
    gemini_models = []
    for model in gemini["models"]:
        model_id = model["name"].removeprefix("models/")
        methods = sorted(model.get("supportedGenerationMethods", []))
        gemini_models.append(
            {
                "id": model_id,
                "display_name": model.get("displayName", ""),
                "supports_generate_content": "generateContent" in methods,
                "supported_generation_methods": methods,
                "input_token_limit": model.get("inputTokenLimit"),
                "output_token_limit": model.get("outputTokenLimit"),
                "free_tier": gemini_pricing_status(model.get("displayName", ""), pricing_sections),
            }
        )
    free_rows = groq_free_rows(groq_limits_html or "")
    groq_models = []
    for model in groq["models"]:
        model_id = model["id"]
        groq_models.append(
            {
                "id": model_id,
                "active": model.get("active"),
                "input_modalities": model.get("input_modalities", []),
                "output_modalities": model.get("output_modalities", []),
                "context_window": model.get("context_window"),
                "max_completion_tokens": model.get("max_completion_tokens"),
                "supported_features": model.get("supported_features", []),
                "free_tier": free_rows.get(model_id),
            }
        )

    gemini_ids = {model["id"] for model in gemini_models}
    groq_ids = {model["id"] for model in groq_models}
    google_catalog = catalog_endpoints(catalog, {"google", "gemini-live"})
    groq_catalog = catalog_endpoints(catalog, {"groq"})
    return {
        "version": 1,
        "observed_at": datetime.now(timezone.utc).isoformat(),
        "policy": {
            "catalog_mutation": "never",
            "api_visibility": "inventory evidence, not general-purpose or free-tier proof",
            "browser_fallback": "only when APIs and machine-readable first-party pages are unavailable or ambiguous",
        },
        "gemini": {
            **{key: value for key, value in gemini.items() if key != "models"},
            "pricing_authority": GEMINI_PRICING_URL,
            "pricing_document_loaded": gemini_pricing_html is not None,
            "models": gemini_models,
            "catalog_not_listed": sorted(google_catalog - gemini_ids),
            "listed_not_catalog": sorted(gemini_ids - google_catalog),
        },
        "groq": {
            **{key: value for key, value in groq.items() if key != "models"},
            "free_limit_authority": GROQ_FREE_LIMITS_URL,
            "free_limit_document_loaded": groq_limits_html is not None,
            "documented_free_model_ids": sorted(free_rows),
            "models": groq_models,
            "catalog_not_listed": sorted(groq_catalog - groq_ids),
            "listed_not_catalog": sorted(groq_ids - groq_catalog),
            "api_visible_not_documented_free": sorted(groq_ids - set(free_rows)),
            "documented_free_not_api_visible": sorted(set(free_rows) - groq_ids),
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--catalog", type=Path, default=Path("catalog/model_catalog.json"))
    parser.add_argument("--dotenv", type=Path, default=Path(".env"))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    dotenv = read_dotenv(args.dotenv)
    gemini_keys = credential_pool("GEMINI_API_KEY", dotenv)
    groq_keys = credential_pool("GROQ_API_KEY", dotenv)
    gemini = gemini_inventory(gemini_keys)
    groq = groq_inventory(groq_keys)
    try:
        gemini_pricing_html = request_text(GEMINI_PRICING_URL)
    except Exception:
        gemini_pricing_html = None
    try:
        groq_limits_html = request_text(GROQ_FREE_LIMITS_URL)
    except Exception:
        groq_limits_html = None
    catalog = json.loads(args.catalog.read_text(encoding="utf-8"))
    report = build_report(catalog, gemini, groq, gemini_pricing_html, groq_limits_html)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(
        json.dumps(
            {
                "gemini_models": len(report["gemini"]["models"]),
                "groq_models": len(report["groq"]["models"]),
                "gemini_listed_not_catalog": len(report["gemini"]["listed_not_catalog"]),
                "groq_listed_not_catalog": len(report["groq"]["listed_not_catalog"]),
                "groq_free_disagreements": len(report["groq"]["api_visible_not_documented_free"]),
            },
            separators=(",", ":"),
        )
    )


if __name__ == "__main__":
    main()
