#!/usr/bin/env python3
"""Build deterministic, content-addressed Windows model ZIPs.

This script never uploads. It converts the two upstream tar.bz2 model releases
that the host cannot safely unpack into exact ZIP inventories for the
append-only sgt-runtime-bundles release.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
import os
import tarfile
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from zlib_ng import zlib_ng


RELEASE_BASE = (
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/"
    "sgt-runtime-bundles"
)
CHUNK_BYTES = 1024 * 1024
MAX_ENTRIES = 2_048
MAX_EXPANDED_BYTES = 2 * 1024 * 1024 * 1024
ZIP_TIME = (1980, 1, 1, 0, 0, 0)
ZLIB_NG_VERSION = "1.0.0"
STAGING_SUFFIX_CHARACTERS = frozenset("abcdefghijklmnopqrstuvwxyz0123456789_")

if importlib.metadata.version("zlib-ng") != ZLIB_NG_VERSION:
    raise RuntimeError(f"package_windows_models.py requires zlib-ng=={ZLIB_NG_VERSION}")
zipfile.zlib = zlib_ng


@dataclass(frozen=True)
class PackageSpec:
    component_id: str
    version: str
    source_filename: str
    source_url: str
    source_size: int
    source_sha256: str
    top_directory: str


PACKAGES = (
    PackageSpec(
        component_id="kokoro-82m-v1-model",
        version="1.0.0-sherpa-tts-models",
        source_filename="kokoro-multi-lang-v1_0.tar.bz2",
        source_url=(
            "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/"
            "kokoro-multi-lang-v1_0.tar.bz2"
        ),
        source_size=349_418_188,
        source_sha256=(
            "c133d26353d776da730870dac7da07dbfc9a5e3bc80cc5e8e83ab6e823be7046"
        ),
        top_directory="kokoro-multi-lang-v1_0",
    ),
    PackageSpec(
        component_id="supertonic-3-model",
        version="2026.05.11-int8",
        source_filename="sherpa-onnx-supertonic-3-tts-int8-2026-05-11.tar.bz2",
        source_url=(
            "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/"
            "sherpa-onnx-supertonic-3-tts-int8-2026-05-11.tar.bz2"
        ),
        source_size=128_774_318,
        source_sha256=(
            "82fa96f91c4ef8abaae3a14a3f4153facf88bed821d1f7331cec2700f432c427"
        ),
        top_directory="sherpa-onnx-supertonic-3-tts-int8-2026-05-11",
    ),
)


def sha256_file(path: Path) -> tuple[int, str]:
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as handle:
        while chunk := handle.read(CHUNK_BYTES):
            size += len(chunk)
            digest.update(chunk)
    return size, digest.hexdigest()


def checked_relative(member_name: str, top_directory: str) -> str | None:
    raw = PurePosixPath(member_name)
    if raw.is_absolute() or any(part in ("", ".", "..") for part in raw.parts):
        raise ValueError(f"unsafe archive path: {member_name!r}")
    if not raw.parts or raw.parts[0] != top_directory:
        raise ValueError(f"archive entry escaped expected top directory: {member_name!r}")
    relative = PurePosixPath(*raw.parts[1:])
    if not relative.parts:
        return None
    value = relative.as_posix()
    if len(relative.parts) > 32 or len(value) > 512 or "\\" in value:
        raise ValueError(f"archive path exceeds delivery limits: {member_name!r}")
    return value


def zip_info(path: str, _size: int) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(path, ZIP_TIME)
    info.compress_type = zipfile.ZIP_DEFLATED
    info.create_system = 3
    info.external_attr = 0o100644 << 16
    return info


def cleanup_stale_staging(output_dir: Path) -> None:
    """Remove only temp directories created by an interrupted invocation."""
    if not output_dir.exists():
        return
    resolved_output = output_dir.resolve(strict=True)
    prefixes = tuple(f"{spec.component_id}-" for spec in PACKAGES)
    for candidate in output_dir.iterdir():
        matching = next((prefix for prefix in prefixes if candidate.name.startswith(prefix)), None)
        if matching is None:
            continue
        suffix = candidate.name[len(matching) :]
        if len(suffix) != 8 or any(char not in STAGING_SUFFIX_CHARACTERS for char in suffix):
            continue
        if candidate.is_symlink() or not candidate.is_dir():
            raise ValueError(f"unsafe model-package staging entry: {candidate}")
        resolved = candidate.resolve(strict=True)
        if resolved.parent != resolved_output:
            raise ValueError(f"model-package staging escaped output directory: {candidate}")
        entries = list(candidate.iterdir())
        if any(
            entry.name != "package.zip" or entry.is_symlink() or not entry.is_file()
            for entry in entries
        ):
            raise ValueError(f"refusing to clean unrecognized staging contents: {candidate}")
        for entry in entries:
            entry.unlink()
        candidate.rmdir()


def package_one(spec: PackageSpec, source_dir: Path, output_dir: Path) -> dict:
    source = source_dir / spec.source_filename
    size, digest = sha256_file(source)
    if size != spec.source_size or digest != spec.source_sha256:
        raise ValueError(f"{source} does not match its pinned source contract")

    output_dir.mkdir(parents=True, exist_ok=True)
    temp_root = Path(tempfile.mkdtemp(prefix=f"{spec.component_id}-", dir=output_dir))
    temp_zip = temp_root / "package.zip"
    files: list[dict] = []
    seen: set[str] = set()
    seen_folded: set[str] = set()
    expanded = 0
    license_found = False
    try:
        with tarfile.open(source, mode="r:bz2") as archive, zipfile.ZipFile(
            temp_zip,
            mode="x",
            compression=zipfile.ZIP_DEFLATED,
            compresslevel=6,
            allowZip64=True,
        ) as output:
            entry_count = 0
            for member in archive:
                entry_count += 1
                if entry_count > MAX_ENTRIES:
                    raise ValueError(f"{source} has too many entries")
                relative = checked_relative(member.name, spec.top_directory)
                if member.isdir():
                    continue
                if not member.isfile() or relative is None:
                    raise ValueError(f"{source} contains a link or special entry: {member.name}")
                folded = relative.casefold()
                if relative in seen or folded in seen_folded:
                    raise ValueError(f"{source} contains a duplicate Windows path: {relative}")
                seen.add(relative)
                seen_folded.add(folded)
                expanded += member.size
                if expanded > MAX_EXPANDED_BYTES:
                    raise ValueError(f"{source} exceeds the expanded-size limit")
                license_found |= PurePosixPath(relative).name.upper().startswith("LICENSE")
                extracted = archive.extractfile(member)
                if extracted is None:
                    raise ValueError(f"failed to read regular archive entry: {member.name}")
                entry_digest = hashlib.sha256()
                written = 0
                with output.open(
                    zip_info(relative, member.size), mode="w", force_zip64=True
                ) as target:
                    while chunk := extracted.read(CHUNK_BYTES):
                        written += len(chunk)
                        if written > member.size:
                            raise ValueError(f"archive entry grew while packaging: {member.name}")
                        entry_digest.update(chunk)
                        target.write(chunk)
                if written != member.size:
                    raise ValueError(f"archive entry was truncated: {member.name}")
                files.append(
                    {
                        "path": relative,
                        "sizeBytes": written,
                        "sha256": entry_digest.hexdigest(),
                    }
                )

            if entry_count == 0:
                raise ValueError(f"{source} is empty")
            if not license_found:
                raise ValueError(f"{source} has no distributable license notice")

        archive_size, archive_sha = sha256_file(temp_zip)
        maximum_size = spec.source_size + max(1024 * 1024, spec.source_size // 100)
        if archive_size > maximum_size:
            raise ValueError(
                f"{spec.component_id} ZIP is {archive_size} bytes; delivery gate is "
                f"{maximum_size} bytes for source archive {spec.source_size}"
            )
        filename = f"sgt-{spec.component_id}-{spec.version}-{archive_sha[:16]}.zip"
        destination = output_dir / filename
        if destination.exists():
            old_size, old_sha = sha256_file(destination)
            if (old_size, old_sha) != (archive_size, archive_sha):
                raise ValueError(f"refusing to replace different bytes at {destination}")
            temp_zip.unlink()
        else:
            os.replace(temp_zip, destination)
        files.sort(key=lambda value: value["path"])
        return {
            "id": spec.component_id,
            "version": spec.version,
            "architecture": "any",
            "archive": {
                "url": f"{RELEASE_BASE}/{filename}",
                "sizeBytes": archive_size,
                "sha256": archive_sha,
            },
            "installedSizeBytes": sum(item["sizeBytes"] for item in files),
            "files": files,
        }
    finally:
        if temp_root.exists():
            entries = list(temp_root.iterdir())
            for entry in entries:
                if entry.name == "package.zip" and entry.is_file() and not entry.is_symlink():
                    entry.unlink()
            temp_root.rmdir()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    cleanup_stale_staging(args.output_dir)
    deliveries = [package_one(spec, args.source_dir, args.output_dir) for spec in PACKAGES]
    manifest = {"schemaVersion": 1, "models": deliveries}
    manifest_path = args.output_dir / "sgt_windows_model_packages.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(manifest_path)
    for delivery in deliveries:
        archive = delivery["archive"]
        print(
            f"{delivery['id']}: {archive['sizeBytes']} bytes "
            f"sha256={archive['sha256']} files={len(delivery['files'])}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
