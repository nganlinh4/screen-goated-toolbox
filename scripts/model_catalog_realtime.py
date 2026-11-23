"""Realtime-transcription engine metadata: on-device engine and platform availability."""

from __future__ import annotations

PLATFORMS = frozenset({"windows", "android"})
# On-device engines each platform actually implements. Cloud selectors use null.
PLATFORM_ENGINES = {
    "windows": frozenset(),
    "android": frozenset(),
}
ON_DEVICE_ENGINES = frozenset().union(*PLATFORM_ENGINES.values())


def realtime_selector_ids(manifest: dict) -> set[str]:
    """Every canonical selector a saved or listed id can resolve to."""
    ids = set(manifest["realtime_transcription_aliases"].values())
    for options in manifest["realtime_transcription_options"].values():
        ids.update(options)
    return ids


def validate_realtime_engines(manifest: dict) -> None:
    engines = manifest.get("realtime_transcription_engines")
    if not isinstance(engines, dict) or set(engines) != realtime_selector_ids(manifest):
        raise ValueError(
            "realtime_transcription_engines must describe every realtime transcription selector exactly"
        )
    for selector, entry in engines.items():
        if not isinstance(entry, dict) or set(entry) != {"on_device_engine", "platforms"}:
            raise ValueError(f"{selector}: engine entry needs on_device_engine and platforms")
        engine = entry["on_device_engine"]
        platforms = entry["platforms"]
        if engine is not None and engine not in ON_DEVICE_ENGINES:
            raise ValueError(f"{selector}: unknown on-device engine {engine!r}")
        if (
            not isinstance(platforms, list)
            or not platforms
            or len(set(platforms)) != len(platforms)
            or not set(platforms) <= PLATFORMS
        ):
            raise ValueError(f"{selector}: platforms must be a non-empty subset of {sorted(PLATFORMS)}")
        for platform in platforms:
            if engine is not None and engine not in PLATFORM_ENGINES[platform]:
                raise ValueError(f"{selector}: {platform} does not implement {engine}")
    for selector in manifest["realtime_transcription_options"]["windows"]:
        if "windows" not in engines[selector]["platforms"]:
            raise ValueError(f"{selector}: a Windows option must be available on Windows")
