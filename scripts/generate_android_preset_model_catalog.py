#!/usr/bin/env python3

from __future__ import annotations

import argparse
import re
from pathlib import Path


MODEL_ENTRY_RE = re.compile(
    r'ModelConfig::new\(\s*'
    r'"(?P<id>[^"]*)"\s*,\s*'
    r'"(?P<provider>[^"]*)"\s*,\s*'
    r'"(?P<name_vi>[^"]*)"\s*,\s*'
    r'"(?P<name_ko>[^"]*)"\s*,\s*'
    r'"(?P<name_en>[^"]*)"\s*,\s*'
    r'(?:"(?P<full_name_literal>[^"]*)"|(?P<full_name_const>[A-Z0-9_]+))\s*,\s*'
    r'ModelType::(?P<model_type>\w+)\s*,\s*'
    r'(?P<enabled>true|false)',
    re.MULTILINE | re.DOTALL,
)

NON_LLM_CASES_RE = re.compile(r'"([^"]+)"')
CONST_STR_RE = re.compile(r'pub const (?P<name>[A-Z0-9_]+): &str = "(?P<value>[^"]*)";')

PROVIDER_MAP = {
    "google": "GOOGLE",
    "cerebras": "CEREBRAS",
    "groq": "GROQ",
    "openrouter": "OPENROUTER",
    "google-gtx": "GOOGLE_GTX",
    "gemini-live": "GEMINI_LIVE",
    "ollama": "OLLAMA",
    "qrserver": "QRSERVER",
    "parakeet": "PARAKEET",
}

MODEL_TYPE_MAP = {
    "Text": "TEXT",
    "Vision": "VISION",
    "Audio": "AUDIO",
}


def kotlin_string(value: str) -> str:
    escaped = (
        value.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
    )
    return f'"{escaped}"'


def extract_non_llm_ids(source: str) -> set[str]:
    marker = "pub fn model_is_non_llm"
    start = source.find(marker)
    if start < 0:
        return set()
    end = source.find("lazy_static::lazy_static!", start)
    if end < 0:
        end = len(source)
    section = source[start:end]
    return set(NON_LLM_CASES_RE.findall(section))


def extract_string_consts(source: str) -> dict[str, str]:
    return {match.group("name"): match.group("value") for match in CONST_STR_RE.finditer(source)}


def generate_kotlin(source_path: Path, output_path: Path) -> None:
    source = source_path.read_text(encoding="utf-8")
    string_consts = extract_string_consts(source)
    non_llm_ids = extract_non_llm_ids(source)
    entries = list(MODEL_ENTRY_RE.finditer(source))
    if not entries:
        raise SystemExit(f"No ModelConfig::new(...) entries found in {source_path}")

    lines: list[str] = [
        "package dev.screengoated.toolbox.mobile.preset",
        "",
        "// Generated from src/model_config.rs. Do not edit by hand.",
        "internal object GeneratedPresetModelCatalogData {",
        "    val models: List<PresetModelDescriptor> = listOf(",
    ]

    for match in entries:
        provider = match.group("provider")
        model_type = match.group("model_type")
        if provider not in PROVIDER_MAP:
            raise SystemExit(f"Unknown provider mapping for {provider!r}")
        if model_type not in MODEL_TYPE_MAP:
            raise SystemExit(f"Unknown model type mapping for {model_type!r}")

        model_id = match.group("id")
        display_name = match.group("name_en")
        full_name = match.group("full_name_literal")
        if full_name is None:
            const_name = match.group("full_name_const")
            full_name = string_consts.get(const_name or "")
            if full_name is None:
                raise SystemExit(f"Unknown full_name const {const_name!r} in {source_path}")
        enabled = match.group("enabled") == "true"
        if not enabled:
            continue

        lines.extend(
            [
                "        PresetModelDescriptor(",
                f"            id = {kotlin_string(model_id)},",
                f"            provider = PresetModelProvider.{PROVIDER_MAP[provider]},",
                f"            fullName = {kotlin_string(full_name)},",
                f"            modelType = PresetModelType.{MODEL_TYPE_MAP[model_type]},",
                f"            displayName = {kotlin_string(display_name)},",
                f"            isNonLlm = {str(model_id in non_llm_ids).lower()},",
                "        ),",
            ]
        )

    lines.extend(
        [
            "    )",
            "}",
            "",
        ]
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines), encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    generate_kotlin(Path(args.source), Path(args.output))


if __name__ == "__main__":
    main()
