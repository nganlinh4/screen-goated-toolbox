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
    r'(?P<enabled>true|false)\s*,\s*'
    r'"(?P<quota_vi>[^"]*)"\s*,\s*'
    r'"(?P<quota_ko>[^"]*)"\s*,\s*'
    r'"(?P<quota_en>[^"]*)"',
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
    "taalas": "TAALAS",
}

MODEL_TYPE_MAP = {
    "Text": "TEXT",
    "Vision": "VISION",
    "Audio": "AUDIO",
}

PROVIDER_DEFAULTS = {
    "use_groq": True,
    "use_gemini": True,
    "use_openrouter": False,
    "use_cerebras": True,
    "use_ollama": False,
}

EXTRA_MODELS = (
    {
        "id": "google-gemma",
        "provider": "GOOGLE",
        "full_name_const": "REALTIME_TRANSLATION_GEMMA_API_MODEL",
        "model_type": "TEXT",
        "display_name": "Gemma",
        "name_vi": "Gemma",
        "name_ko": "Gemma",
        "quota_en": "20 requests/day",
        "quota_vi": "20 lượt/ngày",
        "quota_ko": "20 요청/일",
        "is_non_llm": False,
    },
)


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


def extract_provider_defaults(config_source: str) -> dict[str, bool]:
    marker = "impl Default for Config"
    start = config_source.find(marker)
    if start < 0:
        return dict(PROVIDER_DEFAULTS)
    section = config_source[start:]
    defaults = dict(PROVIDER_DEFAULTS)
    for key in defaults:
        match = re.search(rf"{key}:\s*(true|false)\s*,", section)
        if match:
            defaults[key] = match.group(1) == "true"
    return defaults


def extract_priority_chain(
    priority_source: str,
    function_name: str,
    string_consts: dict[str, str],
) -> list[str]:
    marker = f"fn {function_name}() -> Vec<String> {{"
    start = priority_source.find(marker)
    if start < 0:
        raise SystemExit(f"Could not locate {function_name} in priority source.")
    end = priority_source.find("}", start)
    if end < 0:
        raise SystemExit(f"Could not parse {function_name} body.")
    section = priority_source[start:end]
    items: list[str] = []
    for raw_line in section.splitlines():
        line = raw_line.strip().rstrip(",")
        if not line:
            continue
        const_match = re.search(r"crate::model_config::([A-Z0-9_]+)\.to_string\(\)", line)
        if const_match:
            const_name = const_match.group(1)
            value = string_consts.get(const_name)
            if value is None:
                raise SystemExit(f"Unknown model_config const {const_name!r} in {function_name}")
            items.append(value)
            continue
        literal_match = re.search(r'"([^"]+)"\.to_string\(\)', line)
        if literal_match:
            items.append(literal_match.group(1))
    if not items:
        raise SystemExit(f"No priority-chain entries found for {function_name}")
    return items


def generate_kotlin(
    model_source_path: Path,
    config_source_path: Path,
    priority_source_path: Path,
    output_path: Path,
) -> None:
    model_source = model_source_path.read_text(encoding="utf-8")
    config_source = config_source_path.read_text(encoding="utf-8")
    priority_source = priority_source_path.read_text(encoding="utf-8")
    string_consts = extract_string_consts(model_source)
    non_llm_ids = extract_non_llm_ids(model_source)
    entries = list(MODEL_ENTRY_RE.finditer(model_source))
    if not entries:
        raise SystemExit(f"No ModelConfig::new(...) entries found in {model_source_path}")
    provider_defaults = extract_provider_defaults(config_source)
    image_to_text_chain = extract_priority_chain(
        priority_source=priority_source,
        function_name="default_image_to_text_priority_chain",
        string_consts=string_consts,
    )
    text_to_text_chain = extract_priority_chain(
        priority_source=priority_source,
        function_name="default_text_to_text_priority_chain",
        string_consts=string_consts,
    )

    lines: list[str] = [
        "package dev.screengoated.toolbox.mobile.preset",
        "",
        "// Generated from src/model_config.rs. Do not edit by hand.",
        "internal object GeneratedPresetModelCatalogData {",
        "    val models: List<PresetModelDescriptor> = listOf(",
    ]

    emitted_ids: set[str] = set()

    for match in entries:
        provider = match.group("provider")
        model_type = match.group("model_type")
        if provider not in PROVIDER_MAP:
            raise SystemExit(f"Unknown provider mapping for {provider!r}")
        if model_type not in MODEL_TYPE_MAP:
            raise SystemExit(f"Unknown model type mapping for {model_type!r}")

        model_id = match.group("id")
        emitted_ids.add(model_id)
        display_name = match.group("name_en")
        name_vi = match.group("name_vi")
        name_ko = match.group("name_ko")
        full_name = match.group("full_name_literal")
        if full_name is None:
            const_name = match.group("full_name_const")
            full_name = string_consts.get(const_name or "")
            if full_name is None:
                raise SystemExit(f"Unknown full_name const {const_name!r} in {source_path}")
        enabled = match.group("enabled") == "true"
        if not enabled:
            continue

        quota_vi = match.group("quota_vi")
        quota_ko = match.group("quota_ko")
        quota_en = match.group("quota_en")

        lines.extend(
            [
                "        PresetModelDescriptor(",
                f"            id = {kotlin_string(model_id)},",
                f"            provider = PresetModelProvider.{PROVIDER_MAP[provider]},",
                f"            fullName = {kotlin_string(full_name)},",
                f"            modelType = PresetModelType.{MODEL_TYPE_MAP[model_type]},",
                f"            displayName = {kotlin_string(display_name)},",
                f"            nameVi = {kotlin_string(name_vi)},",
                f"            nameKo = {kotlin_string(name_ko)},",
                f"            isNonLlm = {str(model_id in non_llm_ids).lower()},",
                f"            quotaEn = {kotlin_string(quota_en)},",
                f"            quotaVi = {kotlin_string(quota_vi)},",
                f"            quotaKo = {kotlin_string(quota_ko)},",
                "        ),",
            ]
        )

    for extra_model in EXTRA_MODELS:
        if extra_model["id"] in emitted_ids:
            continue
        full_name = string_consts.get(extra_model["full_name_const"])
        if full_name is None:
            raise SystemExit(
                f"Unknown full_name const {extra_model['full_name_const']!r} for extra model {extra_model['id']!r}"
            )
        lines.extend(
            [
                "        PresetModelDescriptor(",
                f"            id = {kotlin_string(extra_model['id'])},",
                f"            provider = PresetModelProvider.{extra_model['provider']},",
                f"            fullName = {kotlin_string(full_name)},",
                f"            modelType = PresetModelType.{extra_model['model_type']},",
                f"            displayName = {kotlin_string(extra_model['display_name'])},",
                f"            nameVi = {kotlin_string(extra_model['name_vi'])},",
                f"            nameKo = {kotlin_string(extra_model['name_ko'])},",
                f"            isNonLlm = {str(extra_model['is_non_llm']).lower()},",
                f"            quotaEn = {kotlin_string(extra_model['quota_en'])},",
                f"            quotaVi = {kotlin_string(extra_model['quota_vi'])},",
                f"            quotaKo = {kotlin_string(extra_model['quota_ko'])},",
                "        ),",
            ]
        )

    lines.extend(
        [
        "    )",
        "",
        "    val providerSettings = PresetProviderSettings(",
        f"        useGroq = {str(provider_defaults['use_groq']).lower()},",
        f"        useGemini = {str(provider_defaults['use_gemini']).lower()},",
        f"        useOpenRouter = {str(provider_defaults['use_openrouter']).lower()},",
        f"        useCerebras = {str(provider_defaults['use_cerebras']).lower()},",
        f"        useOllama = {str(provider_defaults['use_ollama']).lower()},",
        "    )",
        "",
        "    val modelPriorityChains = PresetModelPriorityChains(",
        "        imageToText = listOf(",
        *[f"            {kotlin_string(item)}," for item in image_to_text_chain],
        "        ),",
        "        textToText = listOf(",
        *[f"            {kotlin_string(item)}," for item in text_to_text_chain],
        "        ),",
        "    )",
        "}",
        "",
    ]
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines), encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-source", required=True)
    parser.add_argument("--config-source", required=True)
    parser.add_argument("--priority-source", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    generate_kotlin(
        model_source_path=Path(args.model_source),
        config_source_path=Path(args.config_source),
        priority_source_path=Path(args.priority_source),
        output_path=Path(args.output),
    )


if __name__ == "__main__":
    main()
