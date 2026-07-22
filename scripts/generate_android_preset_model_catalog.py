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
    "moonshine": "MOONSHINE",
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
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    validate_manifest(manifest)
    return manifest


def validate_manifest(manifest: dict) -> None:
    models = manifest["models"]
    ids = [model["id"] for model in models]
    if len(ids) != len(set(ids)):
        raise ValueError("model ids must be unique")
    for old, replacement in manifest["model_id_migrations"].items():
        if old == replacement or replacement not in ids:
            raise ValueError(f"invalid model id migration: {old} -> {replacement}")
    enabled_ids = {model["id"] for model in models if model["enabled"]}
    feature_model_chains = manifest["feature_model_chains"]
    for key in ("help_assistant", "computer_control_grounding"):
        chain = feature_model_chains.get(key)
        if not isinstance(chain, list) or len(chain) != 2:
            raise ValueError(f"{key} must define primary and fallback models")
        if len(set(chain)) != len(chain):
            raise ValueError(f"{key} model chain must not contain duplicates")
        unknown = [model_id for model_id in chain if model_id not in enabled_ids]
        if unknown:
            raise ValueError(f"{key} references disabled or unknown models: {unknown}")
    endpoints = manifest["endpoints"]
    allowed = {"stable", "preview", "experimental", "deprecated", "retired"}
    for endpoint, metadata in endpoints.items():
        if metadata.get("lifecycle") not in allowed or not metadata.get("verified_at"):
            raise ValueError(f"invalid lifecycle metadata for {endpoint}")
        replacement = metadata.get("replacement")
        if replacement is not None and replacement not in endpoints:
            raise ValueError(f"unknown endpoint replacement: {replacement}")
        if "live_thinking" in metadata:
            thinking = metadata["live_thinking"]
            if not isinstance(thinking, dict):
                raise ValueError(f"live_thinking for {endpoint} must be an object")
            kind = thinking.get("kind")
            value = thinking.get("value")
            if kind == "budget":
                valid = (
                    isinstance(value, int)
                    and not isinstance(value, bool)
                    and 0 <= value <= 2_147_483_647
                )
                if not valid:
                    raise ValueError(
                        f"live_thinking budget for {endpoint} must be a non-negative 32-bit integer"
                    )
            elif kind == "level":
                if not isinstance(value, str) or not value.strip():
                    raise ValueError(
                        f"live_thinking level for {endpoint} must be a non-empty string"
                    )
            else:
                raise ValueError(f"unsupported live_thinking kind for {endpoint}: {kind!r}")
        if "live_max_output_tokens" in metadata:
            limit = metadata["live_max_output_tokens"]
            valid = (
                isinstance(limit, int)
                and not isinstance(limit, bool)
                and 1 <= limit <= 0xFFFF_FFFF
            )
            if not valid:
                raise ValueError(
                    f"live_max_output_tokens for {endpoint} must be a positive u32"
                )
        if "live_automatic_activity_detection_default" in metadata and not isinstance(
            metadata["live_automatic_activity_detection_default"], bool
        ):
            raise ValueError(
                f"live_automatic_activity_detection_default for {endpoint} must be boolean"
            )
        if "live_protocol" in metadata and (
            not isinstance(metadata["live_protocol"], str)
            or not metadata["live_protocol"].strip()
        ):
            raise ValueError(f"live_protocol for {endpoint} must be a non-empty string")
        if metadata.get("live_protocol") == "native-audio" and (
            "live_thinking" not in metadata
            or "live_max_output_tokens" not in metadata
        ):
            raise ValueError(
                f"native-audio endpoint {endpoint} must define Live thinking and output policy"
            )
    forbidden = {endpoint for endpoint, metadata in endpoints.items()
                 if metadata["lifecycle"] in {"deprecated", "retired"}}
    for key in ("gemini_live_api_model_2_5", "gemini_live_api_model_3_1"):
        endpoint = manifest["constants"][key]
        profile = endpoints.get(endpoint)
        if profile is None:
            raise ValueError(f"{key} must reference a catalog endpoint")
        if profile.get("live_protocol") != "native-audio":
            raise ValueError(f"{key} endpoint must use the native-audio protocol")
    for model in models:
        if model["enabled"] and model["full_name"] in forbidden:
            raise ValueError(f"enabled model uses retired endpoint: {model['full_name']}")
    runtime = [manifest["defaults"]["tts_gemini_live_model"]]
    runtime += [item["api_model"] for item in manifest["tts_gemini_models"]]
    if forbidden.intersection(runtime):
        raise ValueError("deprecated/retired endpoint cannot be a runtime default")


def generate_preset_kotlin(manifest: dict, output_path: Path) -> None:
    constants = manifest["constants"]
    provider_defaults = manifest["provider_defaults"]
    priority_chains = manifest["priority_chains"]
    feature_model_chains = manifest["feature_model_chains"]
    non_llm_ids = set(manifest["non_llm_ids"])
    search_disabled_full_names = manifest["search_disabled_full_names"]

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
            continue  # Skip models with providers not supported on Android (e.g. qwen3)
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
            "    val searchDisabledFullNames: Set<String> = setOf(",
            *[f"        {kotlin_string(item)}," for item in search_disabled_full_names],
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
            "",
            "    val helpAssistantModelChain: List<String> = listOf(",
            *[f"        {kotlin_string(item)}," for item in feature_model_chains["help_assistant"]],
            "    )",
            "",
            "    val computerControlGroundingModelChain: List<String> = listOf(",
            *[f"        {kotlin_string(item)}," for item in feature_model_chains["computer_control_grounding"]],
            "    )",
            "",
            "    fun normalizeModelId(modelId: String): String = when (modelId) {",
            *[
                f"        {kotlin_string(old)} -> {kotlin_string(replacement)}"
                for old, replacement in manifest["model_id_migrations"].items()
            ],
            "        else -> modelId",
            "    }",
            "}",
            "",
        ]
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines), encoding="utf-8")


def generate_preset_defaults_kotlin(manifest: dict, output_path: Path) -> None:
    constants = manifest["constants"]
    preset_defaults = manifest["preset_defaults"]

    lines: list[str] = [
        "package dev.screengoated.toolbox.mobile.shared.preset",
        "",
        "// Generated from catalog/model_catalog.json. Do not edit by hand.",
        f"const val DEFAULT_IMAGE_MODEL_ID = {kotlin_string(constants['default_image_model_id'])}",
        f"const val DEFAULT_TEXT_MODEL_ID = {kotlin_string(constants['default_text_model_id'])}",
    ]

    for const_name, model_id in preset_defaults.items():
        lines.append(f"const val {const_name} = {kotlin_string(model_id)}")

    lines.append("")
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines), encoding="utf-8")


def generate_live_kotlin(manifest: dict, output_path: Path) -> None:
    constants = manifest["constants"]
    defaults = manifest["defaults"]
    aliases = manifest["realtime_transcription_aliases"]
    realtime_options = manifest["realtime_transcription_options"]
    live_translation_providers = manifest["live_translation_providers"]
    tts_gemini_models = manifest["tts_gemini_models"]

    provider_api_by_id = {item["id"]: item["api_model"] for item in live_translation_providers}
    def realtime_option_label(option_id: str) -> str:
        return {
            "gemini-live-audio": "Gemini Live",
            "gemini-live-audio-3.1": "Gemini S2S",
            "gemini-3.5-translate": "Gemini Translate",
            "parakeet": "Parakeet",
            "qwen3-asr-0.6b": "Qwen3-ASR 0.6B",
            "qwen3-asr-1.7b": "Qwen3-ASR 1.7B",
            "zipformer": "Zipformer",
            "moonshine-tiny-streaming": "Moonshine Tiny",
            "moonshine-small-streaming": "Moonshine Small",
            "moonshine-medium-streaming": "Moonshine Medium",
        }.get(option_id, option_id)

    lines: list[str] = [
        "package dev.screengoated.toolbox.mobile.shared.live",
        "",
        "// Generated from catalog/model_catalog.json. Do not edit by hand.",
        "data class GeneratedGeminiLiveModelOption(",
        "    val apiModel: String,",
        "    val label: String,",
        ")",
        "",
        "data class GeneratedRealtimeTranscriptionOption(",
        "    val id: String,",
        "    val label: String,",
        ")",
        "",
        "sealed interface GeneratedLiveThinkingConfig {",
        "    data class Budget(val value: Int) : GeneratedLiveThinkingConfig",
        "    data class Level(val value: String) : GeneratedLiveThinkingConfig",
        "}",
        "",
        "data class GeneratedLiveEndpointProfile(",
        "    val lifecycle: String,",
        "    val thinking: GeneratedLiveThinkingConfig?,",
        "    val maxOutputTokens: Long?,",
        "    val automaticActivityDetectionDefault: Boolean,",
        "    val protocol: String?,",
        ")",
        "",
        "object GeneratedLiveModelCatalog {",
        f"    const val TRANSCRIPTION_GEMINI_2_5 = {kotlin_string(constants['gemini_live_audio_model_id_2_5'])}",
        f"    const val TRANSCRIPTION_GEMINI_3_1 = {kotlin_string(constants['gemini_live_audio_model_id_3_1'])}",
        f"    const val TRANSCRIPTION_GEMINI_S2S = {kotlin_string(constants['gemini_live_s2s_model_id'])}",
        f"    const val TRANSCRIPTION_GEMINI_TRANSLATE = {kotlin_string(constants['gemini_live_translate_model_id'])}",
        '    const val TRANSCRIPTION_PARAKEET = "parakeet"',
        '    const val TRANSCRIPTION_MOONSHINE = "moonshine-local"',
        f"    const val GEMINI_LIVE_API_MODEL_2_5 = {kotlin_string(constants['gemini_live_api_model_2_5'])}",
        f"    const val GEMINI_LIVE_API_MODEL_3_1 = {kotlin_string(constants['gemini_live_api_model_3_1'])}",
        f"    const val GEMINI_LIVE_TRANSLATE_API_MODEL = {kotlin_string(constants['gemini_live_translate_api_model'])}",
        f"    const val DEFAULT_TTS_GEMINI_MODEL = {kotlin_string(defaults['tts_gemini_live_model'])}",
        f"    const val DEFAULT_TRANSCRIPTION_PROVIDER_ID = {kotlin_string(defaults['live_session_transcription_provider_id'])}",
        f"    const val DEFAULT_TRANSLATION_PROVIDER_ID = {kotlin_string(defaults['live_session_translation_provider_id'])}",
        f"    const val TRANSLATION_PROVIDER_LLM = {kotlin_string(constants['realtime_translation_model_llm'])}",
        f"    const val TRANSLATION_PROVIDER_GTX = {kotlin_string(constants['realtime_translation_model_gtx'])}",
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

    lines.extend([
        "    )",
        "",
        "    fun endpointProfile(apiModel: String): GeneratedLiveEndpointProfile? = when (apiModel) {",
    ])
    for endpoint, metadata in manifest["endpoints"].items():
        thinking = metadata.get("live_thinking")
        if thinking is None:
            thinking_value = "null"
        else:
            constructor = "Budget" if thinking["kind"] == "budget" else "Level"
            value = str(thinking["value"]) if thinking["kind"] == "budget" else kotlin_string(thinking["value"])
            thinking_value = f"GeneratedLiveThinkingConfig.{constructor}({value})"
        limit = metadata.get("live_max_output_tokens")
        limit_value = "null" if limit is None else f"{limit}L"
        activity_detection = str(
            metadata.get("live_automatic_activity_detection_default", False)
        ).lower()
        protocol = metadata.get("live_protocol")
        protocol_value = "null" if protocol is None else kotlin_string(protocol)
        lines.append(
            f"        {kotlin_string(endpoint)} -> GeneratedLiveEndpointProfile("
            f"lifecycle = {kotlin_string(metadata['lifecycle'])}, thinking = {thinking_value}, "
            f"maxOutputTokens = {limit_value}, automaticActivityDetectionDefault = {activity_detection}, "
            f"protocol = {protocol_value})"
        )
    lines.extend(["        else -> null", "    }", ""])

    lines.extend([
        "    fun thinkingConfig(apiModel: String): GeneratedLiveThinkingConfig? =",
        "        endpointProfile(apiModel)?.thinking",
        "",
        "    fun maxOutputTokens(apiModel: String): Long? =",
        "        endpointProfile(apiModel)?.maxOutputTokens",
        "",
    ])

    lines.extend(
        [
            "    val realtimeTranscriptionOptions: List<GeneratedRealtimeTranscriptionOption> = listOf(",
        ]
    )

    for option_id in realtime_options["android"]:
        lines.extend(
            [
                "        GeneratedRealtimeTranscriptionOption(",
                f"            id = {kotlin_string(option_id)},",
                f"            label = {kotlin_string(realtime_option_label(option_id))},",
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
            "            TRANSCRIPTION_MOONSHINE,",
            '            "moonshine-tiny-streaming",',
            '            "moonshine-small-streaming",',
            '            "moonshine-medium-streaming",',
            '            "zipformer",',
            "            -> ProviderDescriptor(",
            "                id = modelId,",
            '                model = modelId,',
            "            )",
            "",
            "            TRANSCRIPTION_GEMINI_S2S -> ProviderDescriptor(",
            "                id = TRANSCRIPTION_GEMINI_S2S,",
            "                model = GEMINI_LIVE_API_MODEL_3_1,",
            "            )",
            "",
            "            TRANSCRIPTION_GEMINI_TRANSLATE -> ProviderDescriptor(",
            "                id = TRANSCRIPTION_GEMINI_TRANSLATE,",
            "                model = GEMINI_LIVE_TRANSLATE_API_MODEL,",
            "            )",
            "",
            '            "gemini-live-audio-3.1" -> ProviderDescriptor(',
            '                id = "gemini-live-audio-3.1",',
            "                model = GEMINI_LIVE_API_MODEL_3_1,",
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
            "        return ttsGeminiModels",
            "            .firstOrNull { it.apiModel == apiModel }",
            "            ?.apiModel",
            "            ?: DEFAULT_TTS_GEMINI_MODEL",
            "    }",
            "",
            "    fun translationProviderDescriptor(id: String = DEFAULT_TRANSLATION_PROVIDER_ID): ProviderDescriptor {",
            "        return when (id) {",
            "            TRANSLATION_PROVIDER_GTX -> ProviderDescriptor(",
            "                id = TRANSLATION_PROVIDER_GTX,",
            "                model = GTX_API_MODEL,",
            "            )",
            "",
            "            else -> ProviderDescriptor(",
            "                id = TRANSLATION_PROVIDER_LLM,",
            '                model = "",',
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
    parser.add_argument("--preset-defaults-output")
    parser.add_argument("--live-output")
    parser.add_argument("--validate-only", action="store_true")
    args = parser.parse_args()

    if not args.validate_only and not args.preset_output and not args.preset_defaults_output and not args.live_output:
        raise SystemExit("At least one output must be provided.")

    manifest = load_manifest(Path(args.manifest_source))

    if args.preset_output:
        generate_preset_kotlin(manifest, Path(args.preset_output))
    if args.preset_defaults_output:
        generate_preset_defaults_kotlin(manifest, Path(args.preset_defaults_output))
    if args.live_output:
        generate_live_kotlin(manifest, Path(args.live_output))


if __name__ == "__main__":
    main()
