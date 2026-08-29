from __future__ import annotations

import copy
from typing import Any, Iterator

LOCAL_ONLY_FIELDS = {"assetPath", "sourceUrl", "sources"}
ANDROID_CREATION_SELECTORS = frozenset({"android/full", "android/play"})
ANDROID_SHARED_FIELDS = ("factoryClass", "entries")


def validate_digest(value: Any, label: str) -> str:
    if not isinstance(value, str) or len(value) != 64:
        raise ValueError(f"{label} has an invalid SHA-256")
    if any(character not in "0123456789abcdef" for character in value):
        raise ValueError(f"{label} has a non-lowercase SHA-256")
    return value


def iter_id_nodes(value: Any) -> Iterator[dict[str, Any]]:
    if isinstance(value, dict):
        if isinstance(value.get("id"), str):
            yield value
        for child in value.values():
            yield from iter_id_nodes(child)
    elif isinstance(value, list):
        for child in value:
            yield from iter_id_nodes(child)


def clean_copy(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: clean_copy(child)
            for key, child in value.items()
            if key not in LOCAL_ONLY_FIELDS
        }
    if isinstance(value, list):
        return [clean_copy(child) for child in value]
    return copy.deepcopy(value)


def parse_selectors(values: list[str] | None) -> set[str]:
    selected: set[str] = set()
    for value in values or []:
        parts = value.split("/")
        invalid_part = any(not part or part in {".", ".."} for part in parts)
        if not value or "\\" in value or invalid_part or value in selected:
            raise ValueError(f"invalid or repeated component selector: {value!r}")
        selected.add(value)
    return selected


def asset_name(record: dict[str, Any]) -> str | None:
    name = record.get("asset")
    if isinstance(name, str) and name and "/" not in name and "\\" not in name:
        return name
    url = record.get("url")
    if isinstance(url, str) and "/" in url:
        name = url.rsplit("/", 1)[-1]
        if name and "/" not in name and "\\" not in name:
            return name
    return None


def merge_android_creation_candidate(
    base: dict[str, Any], package: dict[str, Any], selected: set[str]
) -> dict[str, Any]:
    if selected != ANDROID_CREATION_SELECTORS:
        raise ValueError("android/full and android/play must be selected together")
    base_android = base.get("android")
    source_android = package.get("android")
    if not isinstance(base_android, dict) or not isinstance(source_android, dict):
        raise ValueError("Android creation runtime records must be objects")
    for key in ("schemaVersion", "hostVersion", "version", "features"):
        if key in base and key in package and base[key] != package[key]:
            raise ValueError(f"Android package has incompatible {key}")
    for selector in sorted(ANDROID_CREATION_SELECTORS):
        key = selector.rsplit("/", 1)[-1]
        record = source_android.get(key)
        if not isinstance(record, dict) or not asset_name(record):
            raise ValueError(f"package manifest is missing {selector}")
        size = record.get("sizeBytes")
        if not isinstance(size, int) or isinstance(size, bool) or size <= 0:
            raise ValueError(f"{selector} has an invalid size")
        validate_digest(record.get("sha256"), selector)
    factory = source_android.get("factoryClass")
    entries = source_android.get("entries")
    if not isinstance(factory, str) or not factory:
        raise ValueError("package manifest is missing Android factoryClass")
    if (
        not isinstance(entries, list)
        or not entries
        or not all(isinstance(entry, dict) for entry in entries)
    ):
        raise ValueError("package manifest has invalid Android entries")
    for index, entry in enumerate(entries):
        size = entry.get("sizeBytes")
        required = ("archivePath", "installPath", "role")
        if not isinstance(size, int) or isinstance(size, bool) or size <= 0 or any(
            not isinstance(entry.get(key), str) or not entry[key] for key in required
        ):
            raise ValueError(f"Android entry {index} has invalid metadata")
        validate_digest(entry.get("sha256"), f"Android entry {index}")
    candidate = copy.deepcopy(base)
    target_android = candidate["android"]
    for selector in sorted(ANDROID_CREATION_SELECTORS):
        key = selector.rsplit("/", 1)[-1]
        target_android[key] = clean_copy(source_android[key])
    for key in ANDROID_SHARED_FIELDS:
        target_android[key] = clean_copy(source_android[key])
    return candidate


def merge_candidate(
    base: dict[str, Any], package: dict[str, Any], selected: set[str]
) -> dict[str, Any]:
    android_selected = {
        selector
        for selector in selected
        if selector == "android" or selector.startswith("android/")
    }
    if android_selected:
        if android_selected != ANDROID_CREATION_SELECTORS:
            raise ValueError("android/full and android/play must be selected together")
        other_selected = selected - ANDROID_CREATION_SELECTORS
        candidate = (
            merge_candidate(base, package, other_selected)
            if other_selected
            else copy.deepcopy(base)
        )
        return merge_android_creation_candidate(
            candidate, package, set(ANDROID_CREATION_SELECTORS)
        )
    candidate = copy.deepcopy(base)
    if (
        not selected
        and isinstance(candidate.get("components"), list)
        and isinstance(package.get("components"), list)
    ):
        candidate["components"] = clean_copy(package["components"])
    targets = {node["id"]: node for node in iter_id_nodes(candidate)}
    if selected:
        keyed_assets = {
            key
            for key, source in package.items()
            if isinstance(source, dict)
            and "asset" in source
            and isinstance(candidate.get(key), dict)
        }
        missing = selected - targets.keys() - keyed_assets
        if missing:
            raise ValueError(
                "selected components are absent from the tracked contract: "
                + ", ".join(sorted(missing))
            )
    for source in iter_id_nodes(package):
        identifier = source["id"]
        if selected and identifier not in selected:
            continue
        target = targets.get(identifier)
        if target is not None:
            target.update(clean_copy(source))
    for key in ("schemaVersion", "version", "hostVersion", "architecture"):
        if key in package and key in candidate:
            candidate[key] = copy.deepcopy(package[key])
    for key, source in package.items():
        if (
            isinstance(source, dict)
            and "asset" in source
            and isinstance(candidate.get(key), dict)
            and (not selected or key in selected)
        ):
            candidate[key].update(clean_copy(source))
    return candidate
