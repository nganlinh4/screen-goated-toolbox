#!/usr/bin/env python3
"""Verify every SGT-hosted byte identity referenced by an update catalog."""

from __future__ import annotations

import argparse
import json
import urllib.parse
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SOURCES = ROOT / "component-delivery" / "update-catalog-v1.sources.json"
RELEASE_API = (
    "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/"
    "releases/tags/sgt-runtime-bundles"
)
RELEASE_PREFIX = (
    "https://github.com/nganlinh4/screen-goated-toolbox/"
    "releases/download/sgt-runtime-bundles/"
)


def load_json(path: Path) -> object:
    return json.loads(path.read_text(encoding="utf-8"))


def request_json(url: str) -> dict[str, object]:
    request = urllib.request.Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "User-Agent": "SGT-update-catalog-verifier",
        },
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        data = response.read(4 * 1024 * 1024 + 1)
    if len(data) > 4 * 1024 * 1024:
        raise RuntimeError("runtime-bundles release response is too large")
    return json.loads(data)


def hosted_contracts(value: object, source: str) -> list[tuple[str, int, str, str]]:
    found: list[tuple[str, int, str, str]] = []
    if isinstance(value, dict):
        url = value.get("downloadUrl") or value.get("url")
        if isinstance(url, str) and url.startswith(RELEASE_PREFIX):
            size = value.get("sizeBytes")
            if size is None:
                size = value.get("byteCount")
            digest = value.get("sha256")
            if not isinstance(size, int) or size <= 0:
                raise RuntimeError(f"{source}: SGT URL lacks an exact positive size")
            if not isinstance(digest, str) or len(digest) != 64:
                raise RuntimeError(f"{source}: SGT URL lacks an exact SHA-256")
            name = urllib.parse.unquote(url.removeprefix(RELEASE_PREFIX))
            if not name or "/" in name or "\\" in name:
                raise RuntimeError(f"{source}: SGT asset URL has an invalid name")
            found.append((name, size, digest.lower(), source))
        for child in value.values():
            found.extend(hosted_contracts(child, source))
    elif isinstance(value, list):
        for child in value:
            found.extend(hosted_contracts(child, source))
    return found


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sources", type=Path, default=DEFAULT_SOURCES)
    args = parser.parse_args()

    sources = load_json(args.sources)
    if not isinstance(sources, dict) or sources.get("schemaVersion") != 1:
        raise SystemExit("unsupported update-catalog source schema")
    contracts: list[tuple[str, int, str, str]] = []
    for source in sources.get("contracts", []):
        if not isinstance(source, dict) or not isinstance(source.get("path"), str):
            raise SystemExit("invalid update-catalog source entry")
        relative = source["path"]
        path = (ROOT / relative).resolve()
        if ROOT not in path.parents:
            raise SystemExit(f"catalog source escapes repository: {relative}")
        contracts.extend(hosted_contracts(load_json(path), relative))

    release = request_json(RELEASE_API)
    assets = release.get("assets")
    if not isinstance(assets, list) or len(assets) > 256:
        raise SystemExit("runtime-bundles release asset list is invalid")
    by_name = {asset.get("name"): asset for asset in assets if isinstance(asset, dict)}
    seen: dict[str, tuple[int, str]] = {}
    errors: list[str] = []
    for name, size, digest, source in contracts:
        identity = (size, digest)
        if name in seen and seen[name] != identity:
            errors.append(f"conflicting identities for append-only asset: {name}")
            continue
        seen[name] = identity
        asset = by_name.get(name)
        if not isinstance(asset, dict):
            errors.append(f"missing runtime-bundles asset: {name} ({source})")
            continue
        if asset.get("size") != size or asset.get("digest") != f"sha256:{digest}":
            errors.append(f"runtime-bundles identity mismatch: {name} ({source})")
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Verified {len(seen)} append-only runtime-bundles identities.")


if __name__ == "__main__":
    main()
