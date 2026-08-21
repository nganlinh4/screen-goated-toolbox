#!/usr/bin/env python3
"""Verify the published Computer Control engine before emitting host delivery data."""

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
        default="local-runtime-bundles/sgt_computer_control/sgt_computer_control.packages.json",
    )
    parser.add_argument(
        "--output",
        default="local-runtime-bundles/sgt_computer_control/sgt_computer_control.delivery.json",
    )
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    packages_path = require_repo_or_managed_cache(
        repo, repo / args.packages, "Computer Control package manifest"
    )
    output_path = require_repo_or_managed_cache(
        repo, repo / args.output, "Computer Control delivery manifest"
    )
    descriptor = json.loads(packages_path.read_text(encoding="utf-8"))
    component = descriptor["component"]
    release = json.loads(request_bytes(API_URL))
    remote = {asset["name"]: asset for asset in release.get("assets", [])}
    asset = remote.get(component["asset"])
    if asset is None:
        raise RuntimeError(f"release is missing {component['asset']}")
    if asset.get("size") != component["sizeBytes"]:
        raise RuntimeError("published Computer Control engine size does not match")
    body = request_bytes(asset["browser_download_url"])
    if len(body) != component["sizeBytes"]:
        raise RuntimeError("downloaded Computer Control engine size does not match")
    if hashlib.sha256(body).hexdigest() != component["sha256"]:
        raise RuntimeError("downloaded Computer Control engine checksum does not match")
    delivered = {key: value for key, value in component.items() if key != "assetPath"}
    delivered["downloadUrl"] = asset["browser_download_url"]
    delivery = {"schemaVersion": 1, "architecture": "x64", "component": delivered}
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(delivery, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(output_path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
