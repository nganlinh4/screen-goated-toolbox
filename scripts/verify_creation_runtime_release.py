#!/usr/bin/env python3
"""Read back every combined creation-runtime delivery artifact exactly."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import urllib.request
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


def exact_string(value: dict, field: str, label: str) -> str:
    result = value.get(field)
    if not isinstance(result, str) or not result:
        raise RuntimeError(f"{label}.{field} is invalid")
    return result


def delivery_record(value: object, label: str) -> tuple[str, str, int, str]:
    if not isinstance(value, dict):
        raise RuntimeError(f"{label} delivery is invalid")
    asset = exact_string(value, "asset", label)
    url = exact_string(value, "downloadUrl", label)
    digest = exact_string(value, "sha256", label)
    size = value.get("sizeBytes")
    if (
        not all(
            character.isascii() and (character.isalnum() or character in ".-_")
            for character in asset
        )
        or url != RUNTIME_BUNDLES + asset
        or not isinstance(size, int)
        or isinstance(size, bool)
        or not 0 < size <= MAX_ARTIFACT_BYTES
        or len(digest) != 64
        or any(character not in "0123456789abcdef" for character in digest)
    ):
        raise RuntimeError(f"{label} delivery identity is invalid")
    return asset, url, size, digest


def read_back(url: str, size: int, digest: str, label: str) -> None:
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
    if received != size or hasher.hexdigest() != digest:
        raise RuntimeError(f"{label} remote bytes do not match the manifest")


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
    if (
        not isinstance(manifest, dict)
        or manifest.get("schemaVersion") != 1
        or manifest.get("features") != ["image_to_3d"]
        or not isinstance(manifest.get("version"), str)
        or not manifest["version"]
    ):
        raise RuntimeError("combined creation-runtime delivery header is invalid")
    android = manifest.get("android")
    if not isinstance(android, dict):
        raise RuntimeError("combined creation-runtime Android delivery is missing")
    records = [
        ("windows", manifest.get("windows")),
        ("android.full", android.get("full")),
        ("android.play", android.get("play")),
    ]
    seen_assets: set[str] = set()
    failures: list[str] = []
    for label, value in records:
        asset, url, size, digest = delivery_record(value, label)
        if asset in seen_assets:
            raise RuntimeError("combined creation-runtime delivery repeats an asset")
        seen_assets.add(asset)
        try:
            read_back(url, size, digest, label)
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
