#!/usr/bin/env python3
"""Create the deterministic, content-addressed Windows x64 VC runtime pack."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import zipfile

from dev_cache_paths import manifest_path, require_repo_or_managed_cache
from pathlib import Path


COMPONENT_ID = "vc14-x64-runtime"
DEFAULT_VERSION = "14.50.35719.0"
SOURCE_DIR = "src/embed_dlls/x64"
DLL_FILES = (
    "concrt140.dll",
    "msvcp140.dll",
    "msvcp140_1.dll",
    "msvcp140_2.dll",
    "msvcp140_atomic_wait.dll",
    "msvcp140_codecvt_ids.dll",
    "vccorlib140.dll",
    "vcruntime140.dll",
    "vcruntime140_1.dll",
    "vcruntime140_threads.dll",
)
NOTICE_FILES = (
    (
        "component-notices/vc14-x64-runtime/REDIST.txt",
        "licenses/REDIST.txt",
    ),
    (
        "component-notices/vc14-x64-runtime/THIRD-PARTY-NOTICES.txt",
        "licenses/THIRD-PARTY-NOTICES.txt",
    ),
)
FIXED_TIMESTAMP = (1980, 1, 1, 0, 0, 0)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def package_component(repo: Path, output: Path, version: str) -> dict:
    source = repo / SOURCE_DIR
    for name in DLL_FILES:
        path = source / name
        if not path.is_file() or path.stat().st_size == 0:
            raise RuntimeError(f"VC runtime source is missing or empty: {path}")
        if path.read_bytes()[:2] != b"MZ":
            raise RuntimeError(f"VC runtime source is not a PE file: {path}")

    temporary = output / f".{COMPONENT_ID}-{version}.zip.tmp"
    temporary.unlink(missing_ok=True)
    with zipfile.ZipFile(
        temporary,
        mode="w",
        compression=zipfile.ZIP_DEFLATED,
        compresslevel=9,
    ) as archive:
        for name in DLL_FILES:
            relative = f"bin/x64/{name}"
            info = zipfile.ZipInfo(relative, FIXED_TIMESTAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            archive.writestr(info, (source / name).read_bytes())
        for source_path, relative in NOTICE_FILES:
            data = (repo / source_path).read_bytes()
            if not data:
                raise RuntimeError(f"VC runtime notice is empty: {source_path}")
            info = zipfile.ZipInfo(relative, FIXED_TIMESTAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            archive.writestr(info, data)

    archive_bytes = temporary.read_bytes()
    archive_hash = sha256(archive_bytes)
    asset = f"{COMPONENT_ID}-{version}-{archive_hash[:16]}.zip"
    target = output / asset
    if target.exists() and target.read_bytes() != archive_bytes:
        raise RuntimeError(f"refusing to replace existing immutable asset {target}")
    if not target.exists():
        target.write_bytes(archive_bytes)
    temporary.unlink()

    files = []
    for name in DLL_FILES:
        data = (source / name).read_bytes()
        files.append(
            {
                "path": f"bin/x64/{name}",
                "sizeBytes": len(data),
                "sha256": sha256(data),
            }
        )
    for source_path, relative in NOTICE_FILES:
        data = (repo / source_path).read_bytes()
        files.append(
            {
                "path": relative,
                "sizeBytes": len(data),
                "sha256": sha256(data),
            }
        )
    return {
        "id": COMPONENT_ID,
        "asset": asset,
        "assetPath": manifest_path(repo, target),
        "sizeBytes": len(archive_bytes),
        "sha256": archive_hash,
        "unpackedSizeBytes": sum(file["sizeBytes"] for file in files),
        "files": files,
    }


def require_matching_delivery(output: Path, descriptor: dict) -> None:
    delivery_path = output / "sgt_vc_runtime.delivery.json"
    if not delivery_path.is_file():
        raise RuntimeError(
            "verified VC runtime delivery manifest is missing; upload the new immutable pack "
            "and run verify_vc_runtime_release.py before building the release host"
        )
    delivery = json.loads(delivery_path.read_text(encoding="utf-8"))
    expected = descriptor["windows"]["components"]
    actual = delivery.get("windows", {}).get("components", [])
    comparable = ("id", "asset", "sizeBytes", "sha256", "unpackedSizeBytes", "files")
    expected_values = [{key: entry[key] for key in comparable} for entry in expected]
    actual_values = [{key: entry.get(key) for key in comparable} for entry in actual]
    if delivery.get("version") != descriptor["version"] or actual_values != expected_values:
        raise RuntimeError(
            "verified VC runtime delivery manifest does not match the current source DLLs"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", default=DEFAULT_VERSION)
    parser.add_argument(
        "--output-dir", default="local-runtime-bundles/sgt_vc_runtime"
    )
    parser.add_argument("--require-delivery", action="store_true")
    args = parser.parse_args()

    repo = Path(__file__).resolve().parents[1]
    if not re.fullmatch(r"[a-z0-9._-]{1,80}", args.version):
        raise RuntimeError("VC runtime version is not a valid component version")
    output = require_repo_or_managed_cache(repo, repo / args.output_dir, "VC output")
    output.mkdir(parents=True, exist_ok=True)
    component = package_component(repo, output, args.version)
    descriptor = {
        "schemaVersion": 1,
        "version": args.version,
        "windows": {"architecture": "x64", "components": [component]},
    }
    descriptor_path = output / "sgt_vc_runtime.packages.json"
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
