#!/usr/bin/env python3
"""Verify the immutable GitHub VC runtime pack and emit host delivery data."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import urllib.request
from pathlib import Path

from dev_cache_paths import require_repo_or_managed_cache

API_URL = "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/releases/tags/sgt-runtime-bundles"


def request_bytes(url: str) -> bytes:
    request = urllib.request.Request(
        url,
        headers={"Accept": "application/vnd.github+json", "User-Agent": "SGT-release-verifier"},
    )
    with urllib.request.urlopen(request, timeout=120) as response:
        return response.read()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--packages",
        default="local-runtime-bundles/sgt_vc_runtime/sgt_vc_runtime.packages.json",
    )
    parser.add_argument(
        "--output",
        default="local-runtime-bundles/sgt_vc_runtime/sgt_vc_runtime.delivery.json",
    )
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    packages_path = require_repo_or_managed_cache(
        repo, repo / args.packages, "VC package manifest"
    )
    output_path = require_repo_or_managed_cache(
        repo, repo / args.output, "VC delivery manifest"
    )
    descriptor = json.loads(packages_path.read_text(encoding="utf-8"))

    release = json.loads(request_bytes(API_URL))
    remote_assets = {asset["name"]: asset for asset in release.get("assets", [])}
    delivered = []
    for component in descriptor["windows"]["components"]:
        asset_name = component["asset"]
        remote = remote_assets.get(asset_name)
        if remote is None:
            raise RuntimeError(f"release is missing {asset_name}")
        if remote.get("size") != component["sizeBytes"]:
            raise RuntimeError(f"release size mismatch for {asset_name}")
        remote_hash = hashlib.sha256(request_bytes(remote["browser_download_url"])).hexdigest()
        if remote_hash != component["sha256"]:
            raise RuntimeError(f"release checksum mismatch for {asset_name}")
        entry = {key: value for key, value in component.items() if key != "assetPath"}
        entry["downloadUrl"] = remote["browser_download_url"]
        delivered.append(entry)

    delivery = {
        "schemaVersion": 1,
        "version": descriptor["version"],
        "windows": {"architecture": "x64", "components": delivered},
    }
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(delivery, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(output_path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
