#!/usr/bin/env python3
"""Read back immutable local-ASR assets and emit checked host delivery data."""

from __future__ import annotations

import argparse
import hashlib
import json
import urllib.request
from pathlib import Path

OWNER = "nganlinh4"
REPOSITORY = "screen-goated-toolbox"
TAG = "sgt-runtime-bundles"
RELEASE_PREFIX = (
    f"https://github.com/{OWNER}/{REPOSITORY}/releases/download/{TAG}/"
)


def fetch_json(url: str) -> dict:
    request = urllib.request.Request(url, headers={"User-Agent": "ScreenGoatedToolbox"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response)


def hash_remote(url: str, maximum: int) -> tuple[int, str]:
    request = urllib.request.Request(url, headers={"User-Agent": "ScreenGoatedToolbox"})
    digest = hashlib.sha256()
    total = 0
    with urllib.request.urlopen(request, timeout=120) as response:
        while chunk := response.read(1024 * 1024):
            total += len(chunk)
            if total > maximum:
                raise ValueError("published asset exceeds its signed size")
            digest.update(chunk)
    return total, digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--packages",
        type=Path,
        default=Path("local-runtime-bundles/sgt_local_asr/sgt_local_asr.packages.json"),
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("local-runtime-bundles/sgt_local_asr/sgt_local_asr.delivery.json"),
    )
    args = parser.parse_args()
    packages = json.loads(args.packages.read_text(encoding="utf-8"))
    if packages.get("schemaVersion") != 1 or packages.get("architecture") != "x64":
        raise ValueError("unsupported local-ASR packages manifest")
    expected = {entry["asset"]: entry for entry in packages["components"]}
    if len(expected) != 2:
        raise ValueError("local-ASR manifest must contain exactly two components")

    release = fetch_json(
        f"https://api.github.com/repos/{OWNER}/{REPOSITORY}/releases/tags/{TAG}"
    )
    published = {asset["name"]: asset for asset in release.get("assets", [])}
    checked = []
    for name, component in sorted(expected.items()):
        asset = published.get(name)
        if asset is None:
            raise ValueError(f"published asset is missing: {name}")
        if asset.get("size") != component["sizeBytes"]:
            raise ValueError(f"published asset size mismatch: {name}")
        size, digest = hash_remote(asset["browser_download_url"], component["sizeBytes"])
        if size != component["sizeBytes"] or digest != component["sha256"]:
            raise ValueError(f"published asset identity mismatch: {name}")
        delivery = dict(component)
        delivery["downloadUrl"] = RELEASE_PREFIX + name
        checked.append(delivery)

    result = {
        "schemaVersion": 1,
        "windows": {"architecture": "x64", "components": checked},
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
