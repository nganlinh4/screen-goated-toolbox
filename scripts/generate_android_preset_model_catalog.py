#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
from pathlib import Path


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


def kotlin_string(value: str) -> str:
    escaped = (
        value.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
    )
    return f'"{escaped}"'


def load_manifest(manifest_path: Path) -> dict:
    return json.loads(manifest_path.read_text(encoding="utf-8"))


def generate_preset_kotlin(manifest: dict, output_path: Path) -> None:
    constants = manifest["constants"]
    provider_defaults = manifest["provider_defaults"]
    priority_chains = manifest["priority_chains"]
    non_llm_ids = set(manifest["non_llm_ids"])

    lines: list[str] = [
        "package dev.screengoated.toolbox.mobile.preset",
        "",
        "// Generated from catalog/model_catalog.json. Do not edit by hand.",
        "internal object GeneratedPresetModelCatalogData {",
        "    val models: List<PresetModelDescriptor> = listOf(",
    ]

    for model in manifest["models"]:
        provider = model["provider"]
        model_type = model["model_type"]
        if provider not in PROVIDER_MAP:
            raise SystemExit(f"Unknown provider mapping for {provider!r}")
        if model_type not in MODEL_TYPE_MAP:
            raise SystemExit(f"Unknown model type mapping for {model_type!r}")
        if not model["enabled"]:
            continue

        lines.extend(
            [
                "        PresetModelDescriptor(",
                f"            id = {kotlin_string(model['id'])},",
                f"            provider = PresetModelProvider.{PROVIDER_MAP[provider]},",
                f"            fullName = {kotlin_string(model['full_name'])},",
                f"            modelType = PresetModelType.{MODEL_TYPE_MAP[model_type]},",
                f"            displayName = {kotlin_string(model['name_en'])},",
                f"            nameVi = {kotlin_string(model['name_vi'])},",
                f"            nameKo = {kotlin_string(model['name_ko'])},",
                f"            isNonLlm = {str(model['id'] in non_llm_ids).lower()},",
                f"            quotaEn = {kotlin_string(model['quota_en'])},",
                f"            quotaVi = {kotlin_string(model['quota_vi'])},",
                f"            quotaKo = {kotlin_string(model['quota_ko'])},",
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
            *[f"            {kotlin_string(item)}," for item in priority_chains["image_to_text"]],
            "        ),",
            "        textToText = listOf(",
            *[f"            {kotlin_string(item)}," for item in priority_chains["text_to_text"]],
            "        ),",
            "    )",
            "}",
            "",
        ]
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines), encoding="utf-8")


def generate_live_kotlin(manifest: dict, output_path: Path) -> None:
    constants = manifest["constants"]
    defaults = manifest["defaults"]
    aliases = manifest["realtime_transcription_aliases"]
    live_translation_providers = manifest["live_translation_providers"]
    tts_gemini_models = manifest["tts_gemini_models"]

    provider_api_by_id = {item["id"]: item["api_model"] for item in live_translation_providers}

    lines: list[str] = [
        "package dev.screengoated.toolbox.mobile.shared.live",
        "",
        "// Generated from catalog/model_catalog.json. Do not edit by hand.",
        "data class GeneratedGeminiLiveModelOption(",
        "    val apiModel: String,",
        "    val label: String,",
        ")",
        "",
        "object GeneratedLiveModelCatalog {",
        f"    const val TRANSCRIPTION_GEMINI_2_5 = {kotlin_string(constants['gemini_live_audio_model_id_2_5'])}",
        f"    const val TRANSCRIPTION_GEMINI_3_1 = {kotlin_string(constants['gemini_live_audio_model_id_3_1'])}",
        '    const val TRANSCRIPTION_PARAKEET = "parakeet"',
        f"    const val GEMINI_LIVE_API_MODEL_2_5 = {kotlin_string(constants['gemini_live_api_model_2_5'])}",
        f"    const val GEMINI_LIVE_API_MODEL_3_1 = {kotlin_string(constants['gemini_live_api_model_3_1'])}",
        f"    const val DEFAULT_TTS_GEMINI_MODEL = {kotlin_string(defaults['tts_gemini_live_model'])}",
        f"    const val DEFAULT_TRANSCRIPTION_PROVIDER_ID = {kotlin_string(defaults['live_session_transcription_provider_id'])}",
        f"    const val DEFAULT_TRANSLATION_PROVIDER_ID = {kotlin_string(defaults['live_session_translation_provider_id'])}",
        f"    const val TRANSLATION_PROVIDER_CEREBRAS = {kotlin_string(constants['realtime_translation_model_cerebras'])}",
        f"    const val TRANSLATION_PROVIDER_GEMMA = {kotlin_string(constants['realtime_translation_model_gemma'])}",
        f"    const val TRANSLATION_PROVIDER_GTX = {kotlin_string(constants['realtime_translation_model_gtx'])}",
        f"    const val CEREBRAS_API_MODEL = {kotlin_string(provider_api_by_id[constants['realtime_translation_model_cerebras']])}",
        f"    const val GEMMA_API_MODEL = {kotlin_string(provider_api_by_id[constants['realtime_translation_model_gemma']])}",
        f"    const val GTX_API_MODEL = {kotlin_string(provider_api_by_id[constants['realtime_translation_model_gtx']])}",
        "",
        "    val ttsGeminiModels: List<GeneratedGeminiLiveModelOption> = listOf(",
    ]

    for option in tts_gemini_models:
        lines.extend(
            [
                "        GeneratedGeminiLiveModelOption(",
                f"            apiModel = {kotlin_string(option['api_model'])},",
                f"            label = {kotlin_string(option['label'])},",
                "        ),",
            ]
        )

    lines.extend(
        [
            "    )",
            "",
            "    fun normalizeTranscriptionModelId(modelId: String): String {",
            "        return when (modelId) {",
        ]
    )

    for alias, normalized in aliases.items():
        lines.append(f"            {kotlin_string(alias)} -> {kotlin_string(normalized)}")

    lines.extend(
        [
            f"            else -> {kotlin_string(defaults['live_session_transcription_provider_id'])}",
            "        }",
            "    }",
            "",
            "    fun defaultTranscriptionProvider(modelId: String = DEFAULT_TRANSCRIPTION_PROVIDER_ID): ProviderDescriptor {",
            "        return when (normalizeTranscriptionModelId(modelId)) {",
            "            TRANSCRIPTION_PARAKEET -> ProviderDescriptor(",
            "                id = TRANSCRIPTION_PARAKEET,",
            '                model = "realtime_eou_120m-v1-onnx",',
            "            )",
            "",
            "            else -> ProviderDescriptor(",
            "                id = TRANSCRIPTION_GEMINI_2_5,",
            "                model = GEMINI_LIVE_API_MODEL_2_5,",
            "            )",
            "        }",
            "    }",
            "",
            "    fun normalizeTtsGeminiModel(apiModel: String): String {",
            "        return when (apiModel) {",
            '            "",',
            '            "gemini",',
            "            GEMINI_LIVE_API_MODEL_2_5 -> GEMINI_LIVE_API_MODEL_2_5",
            "            GEMINI_LIVE_API_MODEL_3_1 -> GEMINI_LIVE_API_MODEL_3_1",
            "            else -> DEFAULT_TTS_GEMINI_MODEL",
            "        }",
            "    }",
            "",
            "    fun translationProviderDescriptor(id: String = DEFAULT_TRANSLATION_PROVIDER_ID): ProviderDescriptor {",
            "        return when (id) {",
            "            TRANSLATION_PROVIDER_CEREBRAS -> ProviderDescriptor(",
            "                id = TRANSLATION_PROVIDER_CEREBRAS,",
            "                model = CEREBRAS_API_MODEL,",
            "            )",
            "",
            "            TRANSLATION_PROVIDER_GTX -> ProviderDescriptor(",
            "                id = TRANSLATION_PROVIDER_GTX,",
            "                model = GTX_API_MODEL,",
            "            )",
            "",
            "            else -> ProviderDescriptor(",
            "                id = TRANSLATION_PROVIDER_GEMMA,",
            "                model = GEMMA_API_MODEL,",
            "            )",
            "        }",
            "    }",
            "}",
            "",
        ]
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines), encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest-source", required=True)
    parser.add_argument("--preset-output")
    parser.add_argument("--live-output")
    args = parser.parse_args()

    if not args.preset_output and not args.live_output:
        raise SystemExit("At least one output must be provided.")

    manifest = load_manifest(Path(args.manifest_source))

    if args.preset_output:
        generate_preset_kotlin(manifest, Path(args.preset_output))
    if args.live_output:
        generate_live_kotlin(manifest, Path(args.live_output))


if __name__ == "__main__":
    main()
