#!/usr/bin/env python3
"""Create deterministic, content-addressed Windows mini-app asset packs."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import zipfile

from dev_cache_paths import manifest_path, require_repo_or_managed_cache
from pathlib import Path


COMPONENTS = (
    ("creation-3d-web", "src/overlay/three_d_generator/dist"),
    ("prompt-dj-web", "src/overlay/prompt_dj/dist"),
    ("tts-playground-web", "src/overlay/tts_playground/dist"),
)
EXPECTED_FILES = ("assets/index.css", "assets/index.js", "index.html")
FIXED_TIMESTAMP = (1980, 1, 1, 0, 0, 0)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def package_component(repo: Path, output: Path, component_id: str, source_rel: str, version: str) -> dict:
    source = repo / source_rel
    files = sorted(
        path.relative_to(source).as_posix()
        for path in source.rglob("*")
        if path.is_file()
    )
    if files != list(EXPECTED_FILES):
        raise RuntimeError(
            f"{source_rel} must contain exactly {', '.join(EXPECTED_FILES)}; got {files}"
        )

    temporary = output / f".{component_id}-{version}.zip.tmp"
    temporary.unlink(missing_ok=True)
    with zipfile.ZipFile(
        temporary,
        mode="w",
        compression=zipfile.ZIP_DEFLATED,
        compresslevel=9,
    ) as archive:
        for relative in files:
            data = (source / relative).read_bytes()
            info = zipfile.ZipInfo(relative, FIXED_TIMESTAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            archive.writestr(info, data)

    archive_bytes = temporary.read_bytes()
    archive_hash = sha256(archive_bytes)
    asset = f"{component_id}-{version}-{archive_hash[:16]}.zip"
    target = output / asset
    if target.exists() and target.read_bytes() != archive_bytes:
        raise RuntimeError(f"refusing to replace existing immutable asset {target}")
    if not target.exists():
        target.write_bytes(archive_bytes)
    temporary.unlink()

    file_entries = []
    for relative in files:
        data = (source / relative).read_bytes()
        file_entries.append(
            {"path": relative, "sizeBytes": len(data), "sha256": sha256(data)}
        )
    return {
        "id": component_id,
        "asset": asset,
        "assetPath": manifest_path(repo, target),
        "sizeBytes": len(archive_bytes),
        "sha256": archive_hash,
        "unpackedSizeBytes": sum(entry["sizeBytes"] for entry in file_entries),
        "files": file_entries,
    }


def cargo_version(repo: Path) -> str:
    cargo = (repo / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', cargo, re.MULTILINE)
    if not match:
        raise RuntimeError("Cargo.toml package version is missing")
    return match.group(1)


def require_matching_delivery(output: Path, descriptor: dict) -> None:
    delivery_path = output / "sgt_web_assets.delivery.json"
    if not delivery_path.is_file():
        raise RuntimeError(
            "verified web asset delivery manifest is missing; upload the new immutable packs "
            "and run verify_web_asset_release.py before building the release host"
        )
    delivery = json.loads(delivery_path.read_text(encoding="utf-8"))
    expected = descriptor["windows"]["components"]
    actual = delivery.get("windows", {}).get("components", [])
    comparable = ("id", "asset", "sizeBytes", "sha256", "unpackedSizeBytes", "files")
    expected_values = [{key: entry[key] for key in comparable} for entry in expected]
    actual_values = [{key: entry.get(key) for key in comparable} for entry in actual]
    if delivery.get("version") != descriptor["version"] or actual_values != expected_values:
        raise RuntimeError(
            "verified web asset delivery manifest does not match the current frontend bundles"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version")
    parser.add_argument(
        "--output-dir", default="local-runtime-bundles/sgt_web_assets"
    )
    parser.add_argument("--require-delivery", action="store_true")
    args = parser.parse_args()

    repo = Path(__file__).resolve().parents[1]
    version = args.version or cargo_version(repo)
    if not re.fullmatch(r"[a-z0-9._-]{1,80}", version):
        raise RuntimeError("package version is not a valid component version")
    output = require_repo_or_managed_cache(repo, repo / args.output_dir, "web-pack output")
    output.mkdir(parents=True, exist_ok=True)

    entries = [
        package_component(repo, output, component_id, source, version)
        for component_id, source in COMPONENTS
    ]
    descriptor = {
        "schemaVersion": 1,
        "version": version,
        "windows": {"architecture": "x64", "components": entries},
    }
    descriptor_path = output / "sgt_web_assets.packages.json"
    descriptor_path.write_text(
        json.dumps(descriptor, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    if args.require_delivery:
        require_matching_delivery(output, descriptor)
    print(descriptor_path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, zipfile.BadZipFile) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
