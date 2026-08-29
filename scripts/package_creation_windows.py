#!/usr/bin/env python3
"""Create the deterministic Windows Creation product archive."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import zipfile
from pathlib import Path

from dev_cache_paths import manifest_path, require_repo_or_managed_cache


FIXED_TIMESTAMP = (1980, 1, 1, 0, 0, 0)
FEATURES = ("image_to_3d", "image_to_svg", "image_creator")
WEB_FILES = ("assets/index.css", "assets/index.js", "index.html")
WEB_DIST = Path("src/overlay/three_d_generator/dist")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def cargo_version(repo: Path) -> str:
    cargo = (repo / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', cargo, re.MULTILINE)
    if not match:
        raise RuntimeError("Cargo.toml package version is missing")
    return match.group(1)


def runtime_source(repo: Path, manifest_arg: str) -> tuple[Path, str]:
    manifest_path_value = require_repo_or_managed_cache(
        repo, repo / manifest_arg, "Creation runtime manifest"
    )
    manifest = json.loads(manifest_path_value.read_text(encoding="utf-8-sig"))
    if (
        manifest.get("schemaVersion") != 1
        or manifest.get("name") != "sgt_creation_runtime"
        or manifest.get("target") != "windows-x64"
    ):
        raise RuntimeError("Creation runtime manifest has an invalid identity")
    version = manifest.get("version")
    filename = manifest.get("file")
    expected_size = manifest.get("sizeBytes")
    expected_hash = manifest.get("sha256")
    if not isinstance(version, str) or not re.fullmatch(r"[a-z0-9._-]{1,80}", version):
        raise RuntimeError("Creation runtime version is invalid")
    if not isinstance(filename, str) or Path(filename).name != filename:
        raise RuntimeError("Creation runtime filename is unsafe")
    source = manifest_path_value.parent / filename
    data = source.read_bytes()
    if len(data) != expected_size or sha256(data) != expected_hash:
        raise RuntimeError("Creation runtime bytes do not match their build manifest")
    return source, version


def inventory(repo: Path, runtime: Path) -> list[tuple[str, Path]]:
    web_root = repo / WEB_DIST
    actual = sorted(path.relative_to(web_root).as_posix() for path in web_root.rglob("*") if path.is_file())
    if actual != list(WEB_FILES):
        raise RuntimeError(
            f"3D Creation dist must contain exactly {', '.join(WEB_FILES)}; got {actual}"
        )
    entries = [("bin/sgt_creation_runtime.exe", runtime)]
    entries.extend((f"web/{relative}", web_root / relative) for relative in WEB_FILES)
    return sorted(entries)


def write_archive(output: Path, version: str, entries: list[tuple[str, Path]]) -> tuple[Path, bytes]:
    temporary = output / f".creation-windows-{version}.zip.tmp"
    temporary.unlink(missing_ok=True)
    with zipfile.ZipFile(temporary, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for relative, source in entries:
            info = zipfile.ZipInfo(relative, FIXED_TIMESTAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            archive.writestr(info, source.read_bytes())
    data = temporary.read_bytes()
    digest = sha256(data)
    target = output / f"creation-windows-{version}-{digest[:16]}.zip"
    if target.exists() and target.read_bytes() != data:
        raise RuntimeError(f"refusing to replace immutable asset {target}")
    if not target.exists():
        target.write_bytes(data)
    temporary.unlink()
    return target, data


def comparable(value: dict) -> dict:
    keys = (
        "schemaVersion", "hostVersion", "version", "runtimeVersion", "features", "windows"
    )
    result = {key: value.get(key) for key in keys}
    if isinstance(result.get("windows"), dict):
        result["windows"] = {
            key: item
            for key, item in result["windows"].items()
            if key not in {"assetPath", "downloadUrl"}
        }
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version")
    parser.add_argument("--runtime-manifest", required=True)
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--require-delivery", action="store_true")
    args = parser.parse_args()

    repo = Path(__file__).resolve().parents[1]
    version = args.version or cargo_version(repo)
    if not re.fullmatch(r"[a-z0-9._-]{1,80}", version):
        raise RuntimeError("Creation product version is invalid")
    output = require_repo_or_managed_cache(repo, repo / args.output_dir, "Creation package output")
    output.mkdir(parents=True, exist_ok=True)
    runtime, runtime_version = runtime_source(repo, args.runtime_manifest)
    entries = inventory(repo, runtime)
    archive, archive_bytes = write_archive(output, version, entries)

    files = []
    for relative, source in entries:
        data = source.read_bytes()
        files.append({"path": relative, "sizeBytes": len(data), "sha256": sha256(data)})
    descriptor = {
        "schemaVersion": 1,
        "hostVersion": version,
        "version": version,
        "runtimeVersion": runtime_version,
        "features": list(FEATURES),
        "windows": {
            "architecture": "x64",
            "asset": archive.name,
            "assetPath": manifest_path(repo, archive),
            "sizeBytes": len(archive_bytes),
            "sha256": sha256(archive_bytes),
            "unpackedSizeBytes": sum(item["sizeBytes"] for item in files),
            "files": files,
        },
    }
    package_path = output / "sgt_creation_windows.packages.json"
    package_path.write_text(json.dumps(descriptor, indent=2) + "\n", encoding="utf-8")

    if args.require_delivery:
        delivery_path = output / "sgt_creation_windows.delivery.json"
        if not delivery_path.is_file():
            raise RuntimeError("verified Creation delivery manifest is missing")
        delivery = json.loads(delivery_path.read_text(encoding="utf-8"))
        if comparable(delivery) != comparable(descriptor):
            raise RuntimeError("verified Creation delivery does not match the current archive")
    print(package_path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
