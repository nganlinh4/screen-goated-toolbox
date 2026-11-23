#!/usr/bin/env python3
"""Verify every SGT-hosted byte identity referenced by an update catalog.

Release metadata alone does not prove the public download URL serves the
recorded bytes, so every referenced asset is downloaded from its exact URL and
hashed after its release metadata matches. With --published-package the same
read-back covers a freshly uploaded catalog and its signature.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import urllib.parse
import urllib.request
from pathlib import Path

try:
    from update_catalog_payload import build_payload, catalog_bytes, catalog_stem, sequence_errors
except ModuleNotFoundError:  # imported as `scripts.verify_update_catalog_sources` by tests
    from scripts.update_catalog_payload import (
        build_payload,
        catalog_bytes,
        catalog_stem,
        sequence_errors,
    )


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
USER_AGENT = "SGT-update-catalog-verifier"
DOWNLOAD_CHUNK_BYTES = 1024 * 1024


def load_json(path: Path) -> object:
    return json.loads(path.read_text(encoding="utf-8"))


def request_json(url: str) -> dict[str, object]:
    headers = {"Accept": "application/vnd.github+json", "User-Agent": USER_AGENT}
    # Hosted runners share an IP, so the workflow passes its token for the API
    # read. Public asset downloads never carry it.
    if token := os.environ.get("GH_TOKEN"):
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(request, timeout=60) as response:
        data = response.read(4 * 1024 * 1024 + 1)
    if len(data) > 4 * 1024 * 1024:
        raise RuntimeError("runtime-bundles release response is too large")
    return json.loads(data)


Contract = tuple[str, int, str, str, str]


def hosted_contracts(value: object, source: str) -> list[Contract]:
    """Return (asset name, size, sha256, source, exact public URL) entries."""
    found: list[Contract] = []
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
            found.append((name, size, digest.lower(), source, url))
        for child in value.values():
            found.extend(hosted_contracts(child, source))
    elif isinstance(value, list):
        for child in value:
            found.extend(hosted_contracts(child, source))
    return found


def downloaded_identity(url: str, expected_size: int) -> tuple[int, str]:
    """Stream the public URL and return its byte count and SHA-256.

    Reading stops one byte past the expected size so an oversized response
    is reported without being consumed in full.
    """
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    digest = hashlib.sha256()
    size = 0
    with urllib.request.urlopen(request, timeout=120) as response:
        while size <= expected_size:
            chunk = response.read(min(DOWNLOAD_CHUNK_BYTES, expected_size + 1 - size))
            if not chunk:
                break
            size += len(chunk)
            digest.update(chunk)
    return size, digest.hexdigest()


def published_contracts(package: object, source: str) -> list[Contract]:
    """Return the catalog and signature identities a catalog build recorded."""
    if not isinstance(package, dict) or package.get("schemaVersion") != 1:
        raise SystemExit(f"{source}: unsupported catalog package schema")
    contracts: list[Contract] = []
    for key in ("catalog", "signature"):
        entry = package.get(key)
        name = entry.get("asset") if isinstance(entry, dict) else None
        if not isinstance(name, str) or not name or "/" in name or "\\" in name:
            raise SystemExit(f"{source}: {key} asset name is invalid")
        url = RELEASE_PREFIX + urllib.parse.quote(name)
        contracts.extend(
            hosted_contracts({**entry, "downloadUrl": url}, f"{source} {key}")
        )
    return contracts


def verify_contracts(contracts: list[Contract], assets: object) -> tuple[int, list[str]]:
    """Check release metadata, then download and hash every referenced asset once."""
    if not isinstance(assets, list):
        raise SystemExit("runtime-bundles release asset list is invalid")
    by_name = {asset.get("name"): asset for asset in assets if isinstance(asset, dict)}
    seen: dict[str, tuple[int, str]] = {}
    errors: list[str] = []
    for name, size, digest, source, url in contracts:
        identity = (size, digest)
        if name in seen:
            if seen[name] != identity:
                errors.append(f"conflicting identities for append-only asset: {name}")
            continue
        seen[name] = identity
        asset = by_name.get(name)
        if not isinstance(asset, dict):
            errors.append(f"missing runtime-bundles asset: {name} ({source})")
            continue
        if asset.get("size") != size or asset.get("digest") != f"sha256:{digest}":
            errors.append(f"runtime-bundles identity mismatch: {name} ({source})")
            continue
        try:
            served = downloaded_identity(url, size)
        except OSError as error:
            errors.append(f"runtime-bundles download failed: {name} ({source}): {error}")
            continue
        if served != identity:
            errors.append(f"runtime-bundles served bytes mismatch: {name} ({source})")
    return len(seen), errors


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sources", type=Path, default=DEFAULT_SOURCES)
    parser.add_argument(
        "--published-package",
        type=Path,
        help="read back the catalog and signature named by this catalog build output",
    )
    args = parser.parse_args()

    if args.published_package:
        contracts = published_contracts(
            load_json(args.published_package), args.published_package.name
        )
        verified, errors = verify_contracts(contracts, request_json(RELEASE_API).get("assets"))
        if errors:
            raise SystemExit("\n".join(errors))
        print(f"Downloaded and verified {verified} published catalog assets.")
        return

    sources = load_json(args.sources)
    if not isinstance(sources, dict) or sources.get("schemaVersion") != 1:
        raise SystemExit("unsupported update-catalog source schema")
    contracts: list[Contract] = []
    for source in sources.get("contracts", []):
        if not isinstance(source, dict) or not isinstance(source.get("path"), str):
            raise SystemExit("invalid update-catalog source entry")
        relative = source["path"]
        path = (ROOT / relative).resolve()
        if ROOT not in path.parents:
            raise SystemExit(f"catalog source escapes repository: {relative}")
        contracts.extend(hosted_contracts(load_json(path), relative))

    release = request_json(RELEASE_API)
    sequence = int(sources["sequence"])
    expected_catalog = catalog_stem(sequence, catalog_bytes(build_payload(sources))) + ".json"
    published_names = [
        asset.get("name", "") for asset in release.get("assets") or [] if isinstance(asset, dict)
    ]
    errors = sequence_errors(sequence, expected_catalog, published_names)
    verified, asset_errors = verify_contracts(contracts, release.get("assets"))
    errors += asset_errors
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Downloaded and verified {verified} append-only runtime-bundles assets.")


if __name__ == "__main__":
    main()
