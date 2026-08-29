#!/usr/bin/env python3
"""Read back a Windows Creation archive and emit its verified delivery contract."""

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
    request = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json", "User-Agent": "SGT-Creation-release-verifier"})
    with urllib.request.urlopen(request, timeout=120) as response:
        return response.read()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--packages", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    packages_path = require_repo_or_managed_cache(repo, repo / args.packages, "Creation package manifest")
    output_path = require_repo_or_managed_cache(repo, repo / args.output, "Creation delivery manifest")
    descriptor = json.loads(packages_path.read_text(encoding="utf-8"))
    package = descriptor["windows"]

    release = json.loads(request_bytes(API_URL))
    assets = {asset["name"]: asset for asset in release.get("assets", [])}
    remote = assets.get(package["asset"])
    if remote is None:
        raise RuntimeError(f"release is missing {package['asset']}")
    if remote.get("size") != package["sizeBytes"]:
        raise RuntimeError("published Creation archive size does not match")
    remote_hash = hashlib.sha256(request_bytes(remote["browser_download_url"])).hexdigest()
    if remote_hash != package["sha256"]:
        raise RuntimeError("published Creation archive checksum does not match")

    delivered = {key: value for key, value in descriptor.items() if key != "windows"}
    delivered["windows"] = {key: value for key, value in package.items() if key != "assetPath"}
    delivered["windows"]["downloadUrl"] = remote["browser_download_url"]
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(delivered, indent=2) + "\n", encoding="utf-8")
    print(output_path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
