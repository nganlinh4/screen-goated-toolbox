#!/usr/bin/env python3
"""Build the deterministic, delivery-ready Help Assistant data asset."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CATALOG = ROOT / "docs" / "help" / "content-v1.json"
PRODUCTION_PREFIX = (
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/"
    "sgt-runtime-bundles/"
)
MAX_DOCUMENT_BYTES = 128 * 1024
MAX_TOTAL_BYTES = 4 * 1024 * 1024
VALID_PLATFORMS = {"windows", "android"}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_catalog(path: Path) -> tuple[str, list[dict[str, object]]]:
    root = json.loads(path.read_text(encoding="utf-8"))
    if root.get("schemaVersion") != 1:
        raise ValueError("help content catalog uses an unsupported schema")
    version = root.get("contentVersion")
    documents = root.get("documents")
    if not isinstance(version, str) or not version:
        raise ValueError("help content catalog has no contentVersion")
    if not isinstance(documents, list) or not documents:
        raise ValueError("help content catalog has no documents")
    return version, documents


def build_entries(catalog_path: Path, documents: list[dict[str, object]]) -> list[dict[str, object]]:
    entries: list[dict[str, object]] = []
    seen_ids: set[str] = set()
    for document in documents:
        identifier = document.get("id")
        relative = document.get("path")
        title = document.get("title")
        platforms = document.get("platforms")
        if not all(isinstance(value, str) and value for value in (identifier, relative, title)):
            raise ValueError("help content document metadata is invalid")
        if identifier in seen_ids:
            raise ValueError(f"duplicate help content id: {identifier}")
        if (
            not isinstance(platforms, list)
            or not platforms
            or not all(platform in VALID_PLATFORMS for platform in platforms)
        ):
            raise ValueError(f"invalid platforms for help content {identifier}")
        source = (catalog_path.parent / relative).resolve()
        if source.parent != catalog_path.parent.resolve() or not source.is_file():
            raise ValueError(f"help content path escapes its catalog: {relative}")
        text = source.read_text(encoding="utf-8").strip()
        size = len(text.encode("utf-8"))
        if not text or size > MAX_DOCUMENT_BYTES:
            raise ValueError(f"help content document has invalid size: {relative}")
        seen_ids.add(identifier)
        entries.append(
            {
                "id": identifier,
                "path": f"docs/help/{relative}",
                "title": title,
                "platforms": sorted(set(platforms)),
                "text": text,
            }
        )
    return entries


def write_package(output_dir: Path, version: str, entries: list[dict[str, object]]) -> None:
    payload = json.dumps(
        {"schemaVersion": 1, "entries": entries},
        ensure_ascii=False,
        separators=(",", ":"),
    ).encode("utf-8")
    if len(payload) > MAX_TOTAL_BYTES:
        raise ValueError("expanded help index exceeds its delivery boundary")
    compressed = gzip.compress(payload, compresslevel=9, mtime=0)
    digest = sha256(compressed)
    asset = f"help-index-v{version}-{digest[:16]}.json.gz"
    output_dir.mkdir(parents=True, exist_ok=True)
    asset_path = output_dir / asset
    asset_path.write_bytes(compressed)
    manifest = {
        "schemaVersion": 1,
        "version": version,
        "helpIndex": {
            "id": "help-index",
            "asset": asset,
            "downloadUrl": f"{PRODUCTION_PREFIX}{asset}",
            "format": "json-gzip",
            "sizeBytes": len(compressed),
            "sha256": digest,
            "expandedSizeBytes": len(payload),
            "expandedSha256": sha256(payload),
            "entryCount": len(entries),
        },
    }
    manifest_path = output_dir / "help-index-v1.package.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Built {len(entries)} help documents")
    print(f"Expanded: {len(payload)} bytes")
    print(f"Asset: {asset_path}")
    print(f"Package manifest: {manifest_path}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--catalog", type=Path, default=DEFAULT_CATALOG)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    catalog_path = args.catalog.resolve()
    version, documents = read_catalog(catalog_path)
    entries = build_entries(catalog_path, documents)
    write_package(args.output_dir.resolve(), version, entries)


if __name__ == "__main__":
    main()
