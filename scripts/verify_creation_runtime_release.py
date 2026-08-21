#!/usr/bin/env python3
"""Read back every combined creation-runtime delivery artifact exactly."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import sys
import urllib.request
import zipfile
from pathlib import Path


DEFAULT_MANIFEST = (
    "local-runtime-bundles/sgt_creation_runtime/sgt_creation_runtime.delivery.json"
)
RUNTIME_BUNDLES = (
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/"
    "sgt-runtime-bundles/"
)
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_ARTIFACT_BYTES = 8 * 1024 * 1024 * 1024
MAX_ANDROID_ARTIFACT_BYTES = 256 * 1024 * 1024
EXPECTED_FEATURES = frozenset(
    ("image_to_3d", "image_to_svg", "image_creator")
)
ASSET_NAME_FORMATS = {
    "windows": "sgt-creation-runtime-windows-x64-{digest}.exe",
    "android.full": "sgt-creation-runtime-android-arm64-{digest}.zip",
    "android.play": "sgt-creation-runtime-android-{digest}.aar",
}


def exact_string(value: dict, field: str, label: str) -> str:
    result = value.get(field)
    if not isinstance(result, str) or not result:
        raise RuntimeError(f"{label}.{field} is invalid")
    return result


def validate_features(value: object) -> None:
    if not isinstance(value, list) or any(
        not isinstance(feature, str) for feature in value
    ):
        raise RuntimeError("combined creation-runtime delivery features are invalid")
    if len(value) != len(set(value)):
        raise RuntimeError("combined creation-runtime delivery repeats a feature")
    if set(value) != EXPECTED_FEATURES:
        raise RuntimeError(
            "combined creation-runtime delivery features must contain exactly "
            "image_to_3d, image_to_svg, and image_creator"
        )


def expected_asset_name(label: str, digest: str) -> str:
    try:
        template = ASSET_NAME_FORMATS[label]
    except KeyError as error:
        raise RuntimeError(f"unknown creation-runtime delivery target: {label}") from error
    return template.format(digest=digest[:16])


def delivery_record(value: object, label: str) -> tuple[str, str, int, str]:
    if not isinstance(value, dict):
        raise RuntimeError(f"{label} delivery is invalid")
    asset = exact_string(value, "asset", label)
    url = exact_string(value, "downloadUrl", label)
    digest = exact_string(value, "sha256", label)
    size = value.get("sizeBytes")
    if not all(
        character.isascii() and (character.isalnum() or character in ".-_")
        for character in asset
    ):
        raise RuntimeError(f"{label} delivery identity is invalid")
    if (
        not isinstance(size, int)
        or isinstance(size, bool)
        or not 0 < size <= MAX_ARTIFACT_BYTES
        or len(digest) != 64
        or any(character not in "0123456789abcdef" for character in digest)
    ):
        raise RuntimeError(f"{label} delivery identity is invalid")
    if asset != expected_asset_name(label, digest):
        raise RuntimeError(f"{label} delivery asset is not content-addressed")
    if url != RUNTIME_BUNDLES + asset:
        raise RuntimeError(f"{label} delivery URL is not immutable")
    return asset, url, size, digest


def android_entries(android: dict) -> list[tuple[str, str, int, str]]:
    raw_entries = android.get("entries")
    if not isinstance(raw_entries, list) or not raw_entries:
        raise RuntimeError("combined creation-runtime Android entries are invalid")
    entries: list[tuple[str, str, int, str]] = []
    seen_paths: set[str] = set()
    for value in raw_entries:
        if not isinstance(value, dict):
            raise RuntimeError("combined creation-runtime Android entry is invalid")
        archive_path = exact_string(value, "archivePath", "android entry")
        install_path = exact_string(value, "installPath", "android entry")
        role = exact_string(value, "role", "android entry")
        digest = exact_string(value, "sha256", "android entry")
        size = value.get("sizeBytes")
        paths = (archive_path, install_path)
        if (
            role not in {"factory_dex", "native_library"}
            or any(
                "\\" in path
                or path.startswith("/")
                or any(part in {"", ".", ".."} for part in path.split("/"))
                for path in paths
            )
            or archive_path in seen_paths
            or not isinstance(size, int)
            or isinstance(size, bool)
            or size <= 0
            or len(digest) != 64
            or any(character not in "0123456789abcdef" for character in digest)
        ):
            raise RuntimeError("combined creation-runtime Android entry is invalid")
        seen_paths.add(archive_path)
        entries.append((role, archive_path, size, digest))
    if {entry[0] for entry in entries} != {"factory_dex", "native_library"}:
        raise RuntimeError("combined creation-runtime Android entry roles are incomplete")
    return entries


def validate_manifest(manifest: object) -> list[tuple[str, str, str, int, str]]:
    if not isinstance(manifest, dict):
        raise RuntimeError("combined creation-runtime delivery header is invalid")
    schema_version = manifest.get("schemaVersion")
    version = manifest.get("version")
    if (
        not isinstance(schema_version, int)
        or isinstance(schema_version, bool)
        or schema_version != 1
        or not isinstance(version, str)
        or not version
    ):
        raise RuntimeError("combined creation-runtime delivery header is invalid")
    validate_features(manifest.get("features"))
    android = manifest.get("android")
    if not isinstance(android, dict):
        raise RuntimeError("combined creation-runtime Android delivery is missing")
    android_entries(android)
    values = [
        ("windows", manifest.get("windows")),
        ("android.full", android.get("full")),
        ("android.play", android.get("play")),
    ]
    records: list[tuple[str, str, str, int, str]] = []
    seen_assets: set[str] = set()
    for label, value in values:
        asset, url, size, digest = delivery_record(value, label)
        if asset in seen_assets:
            raise RuntimeError("combined creation-runtime delivery repeats an asset")
        seen_assets.add(asset)
        records.append((label, asset, url, size, digest))
    return records


def read_back(url: str, size: int, digest: str, label: str) -> bytes | None:
    capture = label.startswith("android.")
    if capture and size > MAX_ANDROID_ARTIFACT_BYTES:
        raise RuntimeError(f"{label} exceeds the archive inspection boundary")
    captured = io.BytesIO() if capture else None
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "SGT-creation-runtime-release-verifier"},
    )
    with urllib.request.urlopen(request, timeout=180) as response:
        content_length = response.headers.get("Content-Length")
        if content_length is not None and int(content_length) != size:
            raise RuntimeError(
                f"{label} remote size changed: expected {size}, got {content_length}"
            )
        hasher = hashlib.sha256()
        received = 0
        while chunk := response.read(min(1024 * 1024, size - received + 1)):
            received += len(chunk)
            if received > size:
                raise RuntimeError(f"{label} exceeds its exact size")
            hasher.update(chunk)
            if captured is not None:
                captured.write(chunk)
    if received != size or hasher.hexdigest() != digest:
        raise RuntimeError(f"{label} remote bytes do not match the manifest")
    return captured.getvalue() if captured is not None else None


def verify_android_archive(
    label: str,
    payload: bytes,
    entries: list[tuple[str, str, int, str]],
) -> None:
    try:
        with zipfile.ZipFile(io.BytesIO(payload)) as archive:
            selected = entries if label == "android.full" else [
                entry for entry in entries if entry[0] == "native_library"
            ]
            for role, archive_path, size, digest in selected:
                member = (
                    archive_path
                    if label == "android.full"
                    else "jni/" + archive_path.removeprefix("lib/")
                )
                try:
                    info = archive.getinfo(member)
                except KeyError as error:
                    raise RuntimeError(f"{label} is missing {member}") from error
                if info.file_size != size:
                    raise RuntimeError(f"{label} member size changed: {member}")
                with archive.open(info) as source:
                    actual = hashlib.file_digest(source, "sha256").hexdigest()
                if actual != digest:
                    raise RuntimeError(f"{label} member bytes changed: {member}")
                if role == "native_library" and not member.endswith(".so"):
                    raise RuntimeError(f"{label} native member is invalid: {member}")
    except zipfile.BadZipFile as error:
        raise RuntimeError(f"{label} is not a valid archive") from error


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", default=DEFAULT_MANIFEST)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    manifest_path = (repo / args.manifest).resolve()
    manifest_path.relative_to(repo)
    if not manifest_path.is_file() or manifest_path.stat().st_size > MAX_MANIFEST_BYTES:
        raise RuntimeError("combined creation-runtime delivery manifest is missing or unsafe")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    records = validate_manifest(manifest)
    entries = android_entries(manifest["android"])
    failures: list[str] = []
    for label, asset, url, size, digest in records:
        try:
            payload = read_back(url, size, digest, label)
            if payload is not None:
                verify_android_archive(label, payload, entries)
            print(f"verified {label}: {asset} ({size} bytes, {digest})")
        except (OSError, RuntimeError, ValueError) as error:
            failures.append(str(error))
    if failures:
        raise RuntimeError("; ".join(failures))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
