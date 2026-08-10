#!/usr/bin/env python3
"""Build deterministic, content-addressed Moonshine English model bundles."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
import zipfile
from pathlib import Path


FIXED_TIMESTAMP = (1980, 1, 1, 0, 0, 0)
RELEASE_BASE_URL = (
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/"
    "sgt-runtime-bundles"
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def verify_file(path: Path, size_bytes: int, sha256: str) -> None:
    if not path.is_file():
        raise RuntimeError(f"required file is missing: {path}")
    if path.stat().st_size != size_bytes:
        raise RuntimeError(f"size mismatch for {path}")
    if sha256_file(path) != sha256:
        raise RuntimeError(f"SHA-256 mismatch for {path}")


def write_entry(archive: zipfile.ZipFile, source: Path, archive_path: str) -> None:
    info = zipfile.ZipInfo(archive_path, FIXED_TIMESTAMP)
    info.compress_type = zipfile.ZIP_DEFLATED
    info.create_system = 3
    info.external_attr = 0o100644 << 16
    with source.open("rb") as input_stream, archive.open(info, "w") as output_stream:
        shutil.copyfileobj(input_stream, output_stream, length=1024 * 1024)


def package_variant(
    repo: Path,
    models: Path,
    output: Path,
    variant: dict,
    notices: list[dict],
) -> dict:
    variant_id = variant["id"]
    source_dir = models / variant_id
    entries: list[tuple[str, Path, dict]] = []
    for file_contract in variant["files"]:
        source = source_dir / file_contract["path"]
        verify_file(source, file_contract["sizeBytes"], file_contract["sha256"])
        entries.append((file_contract["path"], source, file_contract))
    for notice in notices:
        source = repo / notice["sourcePath"]
        verify_file(source, notice["sizeBytes"], notice["sha256"])
        entries.append((notice["archivePath"], source, notice))

    expected_paths = sorted([entry[0] for entry in entries])
    temporary = output / f".{variant_id}.zip.tmp"
    temporary.unlink(missing_ok=True)
    with zipfile.ZipFile(
        temporary,
        mode="w",
        compression=zipfile.ZIP_DEFLATED,
        compresslevel=9,
        allowZip64=True,
    ) as archive:
        for archive_path, source, _ in sorted(entries):
            write_entry(archive, source, archive_path)

    with zipfile.ZipFile(temporary) as archive:
        actual_paths = sorted(info.filename for info in archive.infolist())
        if actual_paths != expected_paths:
            raise RuntimeError(f"unexpected archive entries for {variant_id}: {actual_paths}")
        if any(info.is_dir() for info in archive.infolist()):
            raise RuntimeError(f"directory entry found in {variant_id} archive")

    archive_hash = sha256_file(temporary)
    asset = f"sgt-moonshine-model-{variant_id}-{archive_hash[:16]}.zip"
    target = output / asset
    if target.exists():
        if target.stat().st_size != temporary.stat().st_size or sha256_file(target) != archive_hash:
            raise RuntimeError(f"refusing to replace immutable asset {target}")
        temporary.unlink()
    else:
        temporary.replace(target)

    return {
        "id": variant_id,
        "asset": asset,
        "assetPath": target.relative_to(repo).as_posix(),
        "downloadUrl": f"{RELEASE_BASE_URL}/{asset}",
        "sizeBytes": target.stat().st_size,
        "sha256": archive_hash,
        "unpackedModelSizeBytes": sum(item["sizeBytes"] for item in variant["files"]),
        "fallbackBaseUrl": variant["fallbackBaseUrl"],
        "files": variant["files"],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--models-dir", required=True)
    parser.add_argument(
        "--output-dir",
        default="local-runtime-bundles/sgt_moonshine_models",
    )
    args = parser.parse_args()

    repo = Path(__file__).resolve().parents[2]
    contract_path = repo / "mobile/native/moonshine-models/model-contract.json"
    contract = json.loads(contract_path.read_text(encoding="utf-8"))
    if contract.get("schemaVersion") != 1:
        raise RuntimeError("unsupported Moonshine model contract schema")
    if contract.get("releaseTag") != "sgt-runtime-bundles":
        raise RuntimeError("Moonshine models must use the append-only runtime-bundles release")

    models = Path(args.models_dir).resolve()
    output = (repo / args.output_dir).resolve()
    output.relative_to(repo)
    output.mkdir(parents=True, exist_ok=True)
    variants = [
        package_variant(repo, models, output, variant, contract["notices"])
        for variant in contract["variants"]
    ]
    descriptor = {
        "schemaVersion": 1,
        "releaseTag": contract["releaseTag"],
        "notices": [
            {
                "path": notice["archivePath"],
                "sizeBytes": notice["sizeBytes"],
                "sha256": notice["sha256"],
            }
            for notice in contract["notices"]
        ],
        "variants": variants,
    }
    descriptor_path = output / "moonshine-models.packages.json"
    descriptor_path.write_text(
        json.dumps(descriptor, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(descriptor_path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
