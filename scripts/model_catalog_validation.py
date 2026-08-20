from __future__ import annotations

import re


PROVIDER_NAME_PREFIXES = {
    "google": "GG",
    "google-gtx": "GG",
    "gemini-live": "GG",
    "groq": "G",
    "openrouter": "O",
    "nvidia": "N",
    "taalas": "T",
    "parakeet": "L",
    "qwen3": "L",
    "qrserver": "QR",
}

PROVIDER_ID_PREFIXES = {
    "google": "google-",
    "google-gtx": "google-",
    "gemini-live": "google-",
    "groq": "groq-",
    "openrouter": "openrouter-",
    "nvidia": "nvidia-",
    "taalas": "taalas-",
    "parakeet": "local-",
    "qwen3": "local-",
    "qrserver": "qrserver-",
}

FORBIDDEN_NAME_TERMS = {
    "vi": (
        " ảnh", " chữ", "ocr", "định vị", "giới hạn", "sắp dừng",
        "dài dòng", "thử nghiệm", "suy luận", "dịch máy", "live lỗi",
    ),
    "ko": (
        "비전", "텍스트", "ocr", "위치 찾기", "제한", "곧 종료",
        "장황", "실험", "추론", "기계 번역", "오류",
    ),
    "en": (
        " vision", " text", "ocr", "grounding", "limited", "retiring",
        "verbose", "experimental", "reasoning", "machine translation",
        "live errors",
    ),
}

PROFILE_FIELDS = (
    "name_vi", "name_ko", "name_en", "quota_vi", "quota_ko", "quota_en",
    "supports_search", "search_tool_enabled_by_default", "intelligence_tier",
    "reasoning_policy",
)


def validate_manifest(manifest: dict) -> None:
    if manifest.get("schema_version") != 8:
        raise ValueError("catalog schema_version must be 8")
    if "model_id_migrations" in manifest:
        raise ValueError("permanent model ID migrations are forbidden")
    models = manifest["models"]
    presentation_variants = manifest["presentation_variants"]
    _validate_presentation_variants(models, presentation_variants)
    _validate_models(models, manifest["model_profiles"], presentation_variants)
    _validate_vision_request_profiles(manifest)
    enabled_ids = {model["id"] for model in models if model["enabled"]}
    _validate_chains(manifest, enabled_ids)
    _validate_endpoints(manifest, models)


def _validate_vision_request_profiles(manifest: dict) -> None:
    expected = {
        f"{model['provider']}:{model['full_name']}"
        for model in manifest["models"]
        if (
            model["enabled"]
            and model["model_type"] == "Vision"
            and model["provider"] in {"google", "groq", "openrouter", "nvidia"}
        )
    }
    request_profiles = manifest.get("vision_request_profiles")
    if not isinstance(request_profiles, dict) or set(request_profiles) != expected:
        raise ValueError(
            "vision_request_profiles must cover every enabled ordinary LLM "
            "vision endpoint exactly"
        )

    for profile_key, request_profile in request_profiles.items():
        if profile_key not in manifest["model_profiles"]:
            raise ValueError(
                f"vision request profile has no model profile: {profile_key}"
            )
        if set(request_profile) != {
            "input_order",
            "media_resolution",
            "sampling",
            "max_output_tokens",
            "structured_output",
        }:
            raise ValueError(
                f"vision request profile fields drifted for {profile_key}"
            )
        if request_profile.get("input_order") not in {"text-first", "image-first"}:
            raise ValueError(f"unsupported input_order for {profile_key}")
        if request_profile.get("media_resolution") != "provider-default":
            raise ValueError(f"unsupported media_resolution for {profile_key}")
        sampling = request_profile.get("sampling")
        if sampling not in {"provider-default", "qwen3-groq-non-thinking"}:
            raise ValueError(f"unsupported sampling policy for {profile_key}")
        max_output_tokens = request_profile.get("max_output_tokens")
        if max_output_tokens is not None and (
            isinstance(max_output_tokens, bool)
            or not isinstance(max_output_tokens, int)
            or not 1 <= max_output_tokens <= 0xFFFF_FFFF
        ):
            raise ValueError(f"invalid max_output_tokens for {profile_key}")
        if request_profile.get("structured_output") not in {
            "unsupported", "prompt-only", "json-object", "strict-json-schema",
        }:
            raise ValueError(f"unsupported structured_output for {profile_key}")
        if sampling == "qwen3-groq-non-thinking" and not (
            profile_key.startswith("groq:")
            and manifest["model_profiles"][profile_key]["reasoning_policy"]
            == "openai-none"
            and max_output_tokens is not None
        ):
            raise ValueError(
                "qwen3-groq-non-thinking requires Groq reasoning policy "
                "openai-none and an output limit"
            )


def _validate_presentation_variants(
    models: list[dict],
    variants: dict[str, dict],
) -> None:
    if not isinstance(variants, dict) or not variants:
        raise ValueError("presentation_variants must be a non-empty object")
    required_fields = {"suffix_vi", "suffix_ko", "suffix_en"}
    for key, variant in variants.items():
        if not isinstance(variant, dict) or set(variant) != required_fields:
            raise ValueError(
                f"presentation variant {key!r} must define only localized suffixes"
            )
        for field in required_fields:
            suffix = variant[field]
            if (
                not isinstance(suffix, str)
                or not suffix.startswith(" (")
                or not suffix.endswith(")")
                or not suffix[2:-1]
                or suffix[2:-1].strip() != suffix[2:-1]
            ):
                raise ValueError(
                    f"{field} for presentation variant {key!r} must be "
                    "a parenthesized suffix"
                )

    used: set[str] = set()
    for model in models:
        key = model.get("presentation_variant")
        if key is None:
            continue
        if not isinstance(key, str) or key not in variants:
            raise ValueError(
                f"unknown presentation variant {key!r} for {model['id']}"
            )
        used.add(key)
        sibling_fields = ("provider", "full_name", "model_type")
        siblings = sum(
            all(candidate.get(field) == model.get(field) for field in sibling_fields)
            for candidate in models
        )
        if siblings <= 1:
            raise ValueError(
                f"presentation variant on {model['id']} requires a behavioral "
                "sibling using the same endpoint"
            )
    if used != set(variants):
        raise ValueError("presentation_variants contains an unreferenced variant")


def _localized_model_name(
    model: dict,
    profile: dict,
    variants: dict[str, dict],
    language: str,
) -> str:
    name = profile[f"name_{language}"]
    key = model.get("presentation_variant")
    if key is not None:
        name += variants[key][f"suffix_{language}"]
    return name


def _validate_models(
    models: list[dict],
    profiles: dict[str, dict],
    presentation_variants: dict[str, dict],
) -> None:
    ids: set[str] = set()
    localized_names: dict[tuple[str, str, str], str] = {}
    used_profiles: set[str] = set()
    for model in models:
        model_id = model["id"]
        if model_id in ids:
            raise ValueError(f"duplicate model id: {model_id}")
        ids.add(model_id)
        _validate_model_id(model)
        full_name = model["full_name"]
        profile_key = f"{model['provider']}:{full_name}"
        profile = profiles.get(profile_key)
        if not isinstance(profile, dict):
            raise ValueError(f"missing model profile for {profile_key}")
        used_profiles.add(profile_key)
        _validate_presentation(model, profile)
        _validate_reasoning_policy(model_id, model["provider"], profile)
        prefix = PROVIDER_NAME_PREFIXES.get(model["provider"])
        if prefix is None:
            raise ValueError(f"missing localized-name prefix for {model['provider']}")
        for language in ("vi", "ko", "en"):
            name = _localized_model_name(
                model,
                profile,
                presentation_variants,
                language,
            )
            if not name.startswith(f"{prefix} "):
                raise ValueError(
                    f"{language} name for {model_id} must start with {prefix}"
                )
            _validate_localized_name(language, model_id, name)
            key = (language, prefix, name)
            existing = localized_names.setdefault(key, profile_key)
            if existing != profile_key:
                raise ValueError(
                    f"distinct API models share {language} name in {prefix}: {name}"
                )
    if used_profiles != set(profiles):
        raise ValueError("model_profiles contains an unreferenced API model")


def _validate_model_id(model: dict) -> None:
    model_id = model["id"]
    segments = model_id.split("-")
    lifecycle_words = {
        "preview", "latest", "experimental", "stable", "deprecated", "retired",
    }
    if (
        re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", model_id) is None
        or segments[0] not in {
            "google", "groq", "openrouter", "nvidia", "taalas", "qrserver", "local",
        }
        or segments[-1] not in {"text", "vision", "audio", "search"}
        or lifecycle_words.intersection(segments)
    ):
        raise ValueError(f"invalid durable model id: {model_id}")
    prefix = PROVIDER_ID_PREFIXES.get(model["provider"])
    if prefix is None or not model_id.startswith(prefix):
        raise ValueError(
            f"model id {model_id} does not match provider {model['provider']}"
        )


def _validate_presentation(model: dict, profile: dict) -> None:
    model_id = model["id"]
    duplicated = set(PROFILE_FIELDS).intersection(model)
    if duplicated:
        raise ValueError(
            f"model row {model_id} duplicates profile fields: {sorted(duplicated)}"
        )
    intelligence = profile.get("intelligence_tier")
    latency = model.get("typical_latency_ms")
    if (
        isinstance(intelligence, bool)
        or not isinstance(intelligence, int)
        or not 1 <= intelligence <= 6
    ):
        raise ValueError(f"intelligence_tier for {model_id} must be 1..6")
    if (
        isinstance(latency, bool)
        or not isinstance(latency, int)
        or not 1 <= latency <= 2_147_483_647
    ):
        raise ValueError(
            f"typical_latency_ms for {model_id} must be a positive cross-platform i32"
        )
    if not isinstance(model.get("performance_source"), str) or not model["performance_source"].strip():
        raise ValueError(f"performance_source for {model_id} must not be empty")
    if not isinstance(profile.get("supports_search"), bool):
        raise ValueError(f"supports_search for {model_id} must be boolean")
    search_enabled = profile.get("search_tool_enabled_by_default")
    if not isinstance(search_enabled, bool):
        raise ValueError(
            f"search_tool_enabled_by_default for {model_id} must be boolean"
        )
    if search_enabled and not profile["supports_search"]:
        raise ValueError(
            f"search_tool_enabled_by_default for {model_id} requires supports_search"
        )
    _validate_quota(model_id, profile)


def _validate_reasoning_policy(model_id: str, provider: str, profile: dict) -> None:
    policy = profile.get("reasoning_policy")
    allowed = {
        "not-applicable",
        "gemini-disabled",
        "gemini-minimal",
        "gemini-low",
        "openai-none",
        "openai-low",
        "provider-managed",
        "live-profile",
    }
    if policy not in allowed:
        raise ValueError(
            f"unsupported reasoning_policy for {model_id!r}: {policy!r}"
        )
    if provider == "google":
        compatible = policy in {"gemini-disabled", "gemini-minimal", "gemini-low"}
    elif provider == "gemini-live":
        compatible = policy == "live-profile"
    elif provider in {"groq", "openrouter", "nvidia"}:
        compatible = policy in {
            "not-applicable",
            "openai-none",
            "openai-low",
            "provider-managed",
        }
    else:
        compatible = policy == "not-applicable"
    if not compatible:
        raise ValueError(
            f"reasoning_policy {policy!r} is incompatible with provider "
            f"{provider!r} for {model_id!r}"
        )


def _validate_localized_name(language: str, model_id: str, name: str) -> None:
    lower = name.lower()
    if any(term in lower for term in FORBIDDEN_NAME_TERMS[language]):
        raise ValueError(
            f"{language} name for {model_id} has forbidden category/lifecycle wording: {name}"
        )


def _validate_quota(model_id: str, profile: dict) -> None:
    labels = (profile["quota_vi"], profile["quota_ko"], profile["quota_en"])
    if labels == ("Không giới hạn", "무제한", "Unlimited"):
        return
    suffixes = (" lượt/ngày", "회/일", " requests/day")
    counts = []
    for label, suffix in zip(labels, suffixes, strict=True):
        if not label.endswith(suffix) or not label.removesuffix(suffix).isdigit():
            raise ValueError(f"invalid daily quota wording for {model_id}: {label}")
        counts.append(int(label.removesuffix(suffix)))
    if len(set(counts)) != 1 or counts[0] <= 0:
        raise ValueError(f"quota counts disagree for {model_id}")


def _validate_chains(manifest: dict, enabled_ids: set[str]) -> None:
    priority_chains = manifest["priority_chains"]
    for key in ("image_to_text", "text_to_text"):
        _validate_chain(priority_chains.get(key), key, enabled_ids)
    constants = manifest["constants"]
    if constants["default_image_model_id"] != priority_chains["image_to_text"][0]:
        raise ValueError("default image model must lead image_to_text")
    if constants["default_text_model_id"] != priority_chains["text_to_text"][0]:
        raise ValueError("default text model must lead text_to_text")
    for key in ("help_assistant", "computer_control_grounding"):
        chain = manifest["feature_model_chains"].get(key)
        _validate_chain(chain, key, enabled_ids)
        if len(chain) != 2:
            raise ValueError(f"{key} must define primary and fallback models")


def _validate_chain(chain: list[str], key: str, enabled_ids: set[str]) -> None:
    if not isinstance(chain, list) or not chain or len(chain) != len(set(chain)):
        raise ValueError(f"{key} must be a non-empty unique model chain")
    unknown = [model_id for model_id in chain if model_id not in enabled_ids]
    if unknown:
        raise ValueError(f"{key} references disabled or unknown models: {unknown}")


def _validate_endpoints(manifest: dict, models: list[dict]) -> None:
    endpoints = manifest["endpoints"]
    allowed = {"stable", "preview", "experimental", "deprecated", "retired"}
    for endpoint, metadata in endpoints.items():
        if metadata.get("lifecycle") not in allowed or not metadata.get("verified_at"):
            raise ValueError(f"invalid lifecycle metadata for {endpoint}")
        replacement = metadata.get("replacement")
        if replacement is not None and replacement not in endpoints:
            raise ValueError(f"unknown endpoint replacement: {replacement}")
        _validate_live_profile(endpoint, metadata)
    forbidden = {
        endpoint for endpoint, metadata in endpoints.items()
        if metadata["lifecycle"] in {"deprecated", "retired"}
    }
    for key in ("gemini_live_api_model_2_5", "gemini_live_api_model_3_1"):
        endpoint = manifest["constants"][key]
        profile = endpoints.get(endpoint)
        if profile is None or profile.get("live_protocol") != "native-audio":
            raise ValueError(f"{key} must reference a native-audio endpoint")
    if any(model["enabled"] and model["full_name"] in forbidden for model in models):
        raise ValueError("enabled model uses a retired endpoint")
    runtime = [manifest["defaults"]["tts_gemini_live_model"]]
    runtime += [item["api_model"] for item in manifest["tts_gemini_models"]]
    if forbidden.intersection(runtime):
        raise ValueError("deprecated/retired endpoint cannot be a runtime default")


def _validate_live_profile(endpoint: str, metadata: dict) -> None:
    thinking = metadata.get("live_thinking")
    if thinking is not None:
        if not isinstance(thinking, dict):
            raise ValueError(f"live_thinking for {endpoint} must be an object")
        kind, value = thinking.get("kind"), thinking.get("value")
        valid = (
            kind == "budget"
            and isinstance(value, int)
            and not isinstance(value, bool)
            and 0 <= value <= 2_147_483_647
        ) or (kind == "level" and isinstance(value, str) and bool(value.strip()))
        if not valid:
            raise ValueError(f"invalid live_thinking for {endpoint}")
    limit = metadata.get("live_max_output_tokens")
    if limit is not None and (
        isinstance(limit, bool) or not isinstance(limit, int) or not 1 <= limit <= 0xFFFF_FFFF
    ):
        raise ValueError(f"invalid live_max_output_tokens for {endpoint}")
    automatic = metadata.get("live_automatic_activity_detection_default")
    if automatic is not None and not isinstance(automatic, bool):
        raise ValueError(f"invalid activity detection default for {endpoint}")
    protocol = metadata.get("live_protocol")
    if protocol is not None and (not isinstance(protocol, str) or not protocol.strip()):
        raise ValueError(f"invalid live_protocol for {endpoint}")
    if protocol == "native-audio" and (thinking is None or limit is None):
        raise ValueError(
            f"native-audio endpoint {endpoint} must define thinking and output policy"
        )
