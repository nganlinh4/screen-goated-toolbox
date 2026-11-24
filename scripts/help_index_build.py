#!/usr/bin/env python3
"""Build the content-addressed Help Assistant source index for a Git revision."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

from catalog_model_discovery import credential_pool, read_dotenv
from help_index_corpus import collect
from help_index_embeddings import DIMENSIONS, attach_embeddings


ROOT = Path(__file__).resolve().parent.parent
PRODUCTION_PREFIX = (
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/"
    "sgt-runtime-bundles/"
)
MAX_EXPANDED_BYTES = 128 * 1024 * 1024
MAX_COMPRESSED_BYTES = 64 * 1024 * 1024
MAX_ENTRIES = 131072


def json_line(value: dict) -> bytes:
    return (json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n").encode()


def write_package(output_dir: Path, revision: str, model: str, entries: list[dict]) -> Path:
    if not 1 <= len(entries) <= MAX_ENTRIES:
        raise ValueError("Help index entry count exceeds its delivery bounds")
    metadata = {
        "schemaVersion": 2,
        "embeddingModel": model,
        "dimensions": DIMENSIONS,
        "sourceRevision": revision,
    }
    expanded_hash = hashlib.sha256()
    expanded_size = 0
    output_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="help-index-", dir=output_dir) as temporary:
        staging = Path(temporary) / "index.gz"
        with staging.open("wb") as destination, gzip.GzipFile(
            filename="", mode="wb", fileobj=destination, compresslevel=6, mtime=0
        ) as gzip_file:
            for record in (metadata, *entries):
                line = json_line(record)
                expanded_size += len(line)
                if expanded_size > MAX_EXPANDED_BYTES:
                    raise ValueError("Expanded help index exceeds its delivery bounds")
                expanded_hash.update(line)
                gzip_file.write(line)
        compressed_size = staging.stat().st_size
        if not 1 <= compressed_size <= MAX_COMPRESSED_BYTES:
            raise ValueError("Compressed help index exceeds its delivery bounds")
        with staging.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        asset = f"help-index-v2-{digest[:16]}.json.gz"
        staging.replace(output_dir / asset)
        manifest = {
            "schemaVersion": 2,
            "version": "2",
            "helpIndex": {
                "id": "help-index",
                "asset": asset,
                "downloadUrl": PRODUCTION_PREFIX + asset,
                "format": "jsonl-gzip",
                "sizeBytes": compressed_size,
                "sha256": digest,
                "expandedSizeBytes": expanded_size,
                "expandedSha256": expanded_hash.hexdigest(),
                "entryCount": len(entries),
            },
        }
        manifest_path = output_dir / "help-index-v2.package.json"
        prepared_manifest = Path(temporary) / manifest_path.name
        prepared_manifest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        prepared_manifest.replace(manifest_path)
    print(f"Built {len(entries)} chunks, {compressed_size:,} compressed bytes", flush=True)
    print(f"Package manifest: {manifest_path}", flush=True)
    return manifest_path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ref", default="HEAD")
    parser.add_argument("--dotenv", type=Path, default=ROOT / ".env")
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--embedding-cache", type=Path)
    parser.add_argument("--workers", type=int, default=4)
    args = parser.parse_args()
    if not 1 <= args.workers <= 20:
        parser.error("--workers must be between 1 and 20")
    keys = credential_pool("GEMINI_API_KEY", read_dotenv(args.dotenv))
    if not keys:
        parser.error("Set GEMINI_API_KEY or configure the local Gemini key pool")
    revision = subprocess.check_output(
        ["git", "rev-parse", "--verify", "--end-of-options", f"{args.ref}^{{commit}}"], cwd=ROOT, text=True
    ).strip()
    catalog = json.loads(
        subprocess.check_output(["git", "show", f"{revision}:catalog/model_catalog.json"], cwd=ROOT)
    )
    model = catalog["constants"]["gemini_embedding_api_model"]
    entries, counts = collect(ROOT, revision)
    print(
        f"{revision[:12]}: {counts['files']} files, {len(entries)} chunks, "
        f"{counts['source_chars']:,} source characters, "
        f"{counts['credential_match']} files excluded by credential scan, "
        f"{counts['redacted_url_credentials']} URL credentials redacted",
        flush=True,
    )
    output = args.output_dir.resolve()
    cache = args.embedding_cache or output / "embeddings.sqlite3"
    cache.parent.mkdir(parents=True, exist_ok=True)
    attach_embeddings(entries, model, keys, cache, min(args.workers, len(keys)))
    write_package(output, revision, model, entries)


if __name__ == "__main__":
    main()
