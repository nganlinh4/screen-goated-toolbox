#!/usr/bin/env python3
"""Read back every immutable Qwen3 pack and emit build-time delivery metadata."""

from __future__ import annotations

import argparse
import json
import sys
import urllib.request
from pathlib import Path


API_URL = (
    "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/"
    "releases/tags/sgt-runtime-bundles"
)


def request(url: str, method: str = "GET"):
    return urllib.request.urlopen(
        urllib.request.Request(
            url,
            method=method,
            headers={
                "Accept": "application/vnd.github+json",
                "User-Agent": "SGT-release-verifier",
            },
        ),
        timeout=180,
    )


def request_bytes(url: str) -> bytes:
    with request(url) as response:
        return response.read()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--packages",
        default="local-runtime-bundles/sgt_qwen3_runtime/sgt_qwen3_runtime.packages.json",
    )
    parser.add_argument(
        "--output",
        default="local-runtime-bundles/sgt_qwen3_runtime/sgt_qwen3_runtime.delivery.json",
    )
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    packages_path = (repo / args.packages).resolve()
    output_path = (repo / args.output).resolve()
    packages_path.relative_to(repo)
    output_path.relative_to(repo)
    descriptor = json.loads(packages_path.read_text(encoding="utf-8"))
    component = descriptor["windows"]["components"][0]
    release = json.loads(request_bytes(API_URL))
    remote_assets = {asset["name"]: asset for asset in release.get("assets", [])}
    for asset in component["assets"]:
        remote = remote_assets.get(asset["asset"])
        if remote is None:
            raise RuntimeError(f"release is missing {asset['asset']}")
        if remote.get("size") != asset["sizeBytes"]:
            raise RuntimeError(f"release size mismatch for {asset['asset']}")
        if remote.get("digest") != f"sha256:{asset['sha256']}":
            raise RuntimeError(f"release checksum mismatch for {asset['asset']}")
        asset["downloadUrl"] = remote["browser_download_url"]
        asset.pop("assetPath")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(descriptor, indent=2) + "\n", encoding="utf-8")
    print(output_path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
