#!/usr/bin/env python3
"""Verify Moonshine model bundles locally or on the append-only release."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
import tempfile
import urllib.request
import zipfile
from pathlib import Path


API_URL = (
    "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/"
    "releases/tags/sgt-runtime-bundles"
)
USER_AGENT = "SGT-release-verifier"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def request_json(url: str) -> dict:
    request = urllib.request.Request(
        url,
        headers={"Accept": "application/vnd.github+json", "User-Agent": USER_AGENT},
    )
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.load(response)


def download(url: str, target: Path) -> None:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=300) as response, target.open("wb") as output:
        shutil.copyfileobj(response, output, length=1024 * 1024)


def verify_archive(path: Path, delivery: dict, source: dict, notices: list[dict]) -> None:
    if path.stat().st_size != delivery["sizeBytes"]:
        raise RuntimeError(f"archive size mismatch for {delivery['asset']}")
    if sha256_file(path) != delivery["sha256"]:
        raise RuntimeError(f"archive SHA-256 mismatch for {delivery['asset']}")
    expected = {entry["path"]: entry for entry in source["files"]}
    expected.update({entry["archivePath"]: entry for entry in notices})
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        names = [entry.filename for entry in entries]
        if len(names) != len(set(names)) or set(names) != set(expected):
            raise RuntimeError(f"archive entries mismatch for {delivery['asset']}")
        if any(entry.is_dir() for entry in entries):
            raise RuntimeError(f"directory entry found in {delivery['asset']}")
        for entry in entries:
            contract = expected[entry.filename]
            if entry.file_size != contract["sizeBytes"]:
                raise RuntimeError(f"entry size mismatch for {entry.filename}")
            digest = hashlib.sha256()
            with archive.open(entry) as stream:
                while chunk := stream.read(1024 * 1024):
                    digest.update(chunk)
            if digest.hexdigest() != contract["sha256"]:
                raise RuntimeError(f"entry SHA-256 mismatch for {entry.filename}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--local-only", action="store_true")
    parser.add_argument(
        "--local-dir",
        default="local-runtime-bundles/sgt_moonshine_models",
    )
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    source_path = repo / "mobile/native/moonshine-models/model-contract.json"
    delivery_path = (
        repo / "mobile/androidApp/src/main/assets/moonshine-model-delivery.json"
    )
    source_bytes = source_path.read_bytes()
    source = json.loads(source_bytes)
    delivery = json.loads(delivery_path.read_text(encoding="utf-8"))
    if hashlib.sha256(source_bytes).hexdigest() != delivery["modelContractSha256"]:
        raise RuntimeError("pinned model source contract SHA-256 mismatch")
    if source["releaseTag"] != delivery["releaseTag"] or delivery["releaseTag"] != "sgt-runtime-bundles":
        raise RuntimeError("Moonshine delivery must use the append-only runtime-bundles release")

    source_variants = {entry["id"]: entry for entry in source["variants"]}
    delivered_variants = {entry["id"]: entry for entry in delivery["variants"]}
    if source_variants.keys() != delivered_variants.keys():
        raise RuntimeError("source and delivery variants differ")
    release_assets = {}
    if not args.local_only:
        release_assets = {entry["name"]: entry for entry in request_json(API_URL)["assets"]}

    local_dir = (repo / args.local_dir).resolve()
    if args.local_only:
        local_dir.relative_to(repo)
    with tempfile.TemporaryDirectory(prefix="sgt-moonshine-verify-") as temporary:
        for variant_id, delivered in delivered_variants.items():
            asset = delivered["asset"]
            if delivered["sha256"][:16] not in asset:
                raise RuntimeError(f"asset is not content-addressed: {asset}")
            if not delivered["downloadUrl"].endswith(f"/sgt-runtime-bundles/{asset}"):
                raise RuntimeError(f"asset URL is not immutable: {asset}")
            if args.local_only:
                archive_path = local_dir / asset
            else:
                remote = release_assets.get(asset)
                if remote is None:
                    raise RuntimeError(f"release is missing {asset}")
                if remote["size"] != delivered["sizeBytes"]:
                    raise RuntimeError(f"release size mismatch for {asset}")
                archive_path = Path(temporary) / asset
                download(remote["browser_download_url"], archive_path)
            verify_archive(
                archive_path,
                delivered,
                source_variants[variant_id],
                source["notices"],
            )
            print(f"verified {asset}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, KeyError, json.JSONDecodeError, zipfile.BadZipFile) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
