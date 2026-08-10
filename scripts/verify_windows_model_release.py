#!/usr/bin/env python3
"""Verify immutable Windows model delivery without publishing anything."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import sys
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath
from urllib.parse import unquote, urlparse

from generate_windows_model_delivery import generate_delivery


CHUNK_BYTES = 1024 * 1024
MAX_RELEASE_ASSET_BYTES = 2 * 1024 * 1024 * 1024 - 1
MAX_FILES = 4096
REMOTE_FULL_HASH_BYTES = 16 * 1024 * 1024


def sha256_file(path: Path) -> tuple[int, str]:
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as handle:
        while chunk := handle.read(CHUNK_BYTES):
            size += len(chunk)
            digest.update(chunk)
    return size, digest.hexdigest()


def checked_path(value: str) -> str:
    path = PurePosixPath(value)
    if (
        not value
        or path.is_absolute()
        or "\\" in value
        or any(part in ("", ".", "..") for part in path.parts)
        or len(path.parts) > 32
        or len(value) > 512
    ):
        raise ValueError(f"unsafe model path: {value!r}")
    return path.as_posix()


def validate_file_inventory(model: dict) -> dict[str, dict]:
    files = model.get("files")
    if not isinstance(files, list) or not files or len(files) > MAX_FILES:
        raise ValueError(f"{model.get('id')} has an invalid file inventory")
    inventory: dict[str, dict] = {}
    folded: set[str] = set()
    total = 0
    for item in files:
        path = checked_path(item["path"])
        if path in inventory or path.casefold() in folded:
            raise ValueError(f"{model['id']} has a duplicate Windows path: {path}")
        size = item["sizeBytes"]
        digest = item["sha256"]
        if not isinstance(size, int) or size <= 0:
            raise ValueError(f"{model['id']} has an invalid size for {path}")
        if len(digest) != 64 or any(char not in "0123456789abcdef" for char in digest):
            raise ValueError(f"{model['id']} has an invalid SHA-256 for {path}")
        url = item.get("url")
        if url is not None:
            validate_url(url)
        inventory[path] = item
        folded.add(path.casefold())
        total += size
    if total != model.get("installedSizeBytes"):
        raise ValueError(f"{model['id']} installed-size total is inconsistent")
    return inventory


def validate_url(url: str, content_hash: str | None = None) -> None:
    lower = url.lower()
    forbidden = ("/resolve/main/", "/resolve/master/", "/latest/", "nightly", "?")
    if not lower.startswith("https://") or any(value in lower for value in forbidden):
        raise ValueError(f"mutable model URL: {url}")
    if content_hash is not None and content_hash[:16] not in lower:
        raise ValueError(f"release URL is not content-addressed: {url}")


def verify_zip(path: Path, model: dict) -> None:
    inventory = validate_file_inventory(model)
    seen: set[str] = set()
    expanded = 0
    with zipfile.ZipFile(path, "r") as archive:
        entries = archive.infolist()
        if len(entries) != len(inventory) or len(entries) > MAX_FILES:
            raise ValueError(f"{path} has an unexpected entry count")
        for info in entries:
            name = checked_path(info.filename)
            mode = (info.external_attr >> 16) & 0o170000
            if info.is_dir() or mode not in (0, 0o100000):
                raise ValueError(f"{path} contains a directory, link, or special file")
            expected = inventory.get(name)
            if expected is None or name in seen or info.file_size != expected["sizeBytes"]:
                raise ValueError(f"{path} entry does not match inventory: {name}")
            digest = hashlib.sha256()
            size = 0
            with archive.open(info, "r") as handle:
                while chunk := handle.read(CHUNK_BYTES):
                    size += len(chunk)
                    if size > expected["sizeBytes"]:
                        raise ValueError(f"{path} entry exceeds pinned size: {name}")
                    digest.update(chunk)
            if size != expected["sizeBytes"] or digest.hexdigest() != expected["sha256"]:
                raise ValueError(f"{path} entry checksum mismatch: {name}")
            seen.add(name)
            expanded += size
    if seen != set(inventory) or expanded != model["installedSizeBytes"]:
        raise ValueError(f"{path} inventory is incomplete")


def archive_path(package_dir: Path, archive: dict) -> Path:
    filename = Path(unquote(urlparse(archive["url"]).path)).name
    if not filename or filename != Path(filename).name:
        raise ValueError("model archive URL has an invalid filename")
    return package_dir / filename


def verify_local_packages(packages: dict, package_dir: Path) -> None:
    models = packages.get("models")
    if packages.get("schemaVersion") != 1 or not isinstance(models, list):
        raise ValueError("unsupported local package manifest")
    for model in models:
        archive = model["archive"]
        validate_url(archive["url"], archive["sha256"])
        if archive["sizeBytes"] >= MAX_RELEASE_ASSET_BYTES:
            raise ValueError(f"{model['id']} exceeds the release per-file size gate")
        path = archive_path(package_dir, archive)
        size, digest = sha256_file(path)
        if (size, digest) != (archive["sizeBytes"], archive["sha256"]):
            raise ValueError(f"{path} does not match its package receipt")
        verify_zip(path, model)
        print(f"local {model['id']}: {size} bytes sha256={digest}")


def remote_headers(url: str, byte_range: bool) -> tuple[object, bytes]:
    headers = {"User-Agent": "ScreenGoatedToolbox-ReleaseVerifier/1"}
    if byte_range:
        headers["Range"] = "bytes=0-0"
    response = urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=60)
    return response, response.read(1 if byte_range else CHUNK_BYTES)


def header_digest(headers: object) -> str:
    for name in ("x-linked-etag", "etag"):
        value = headers.get(name)
        if value:
            return value.strip().strip('W/').strip('"').lower()
    return ""


def verify_remote(url: str, size: int, sha256: str, force_full: bool) -> None:
    if force_full or size <= REMOTE_FULL_HASH_BYTES:
        request = urllib.request.Request(
            url, headers={"User-Agent": "ScreenGoatedToolbox-ReleaseVerifier/1"}
        )
        digest = hashlib.sha256()
        received = 0
        with urllib.request.urlopen(request, timeout=60) as response:
            while chunk := response.read(CHUNK_BYTES):
                received += len(chunk)
                if received > size:
                    raise ValueError(f"remote object exceeds pinned size: {url}")
                digest.update(chunk)
        if (received, digest.hexdigest()) != (size, sha256):
            raise ValueError(f"remote object checksum mismatch: {url}")
        return

    response, _ = remote_headers(url, True)
    with response:
        headers = response.headers
        content_range = headers.get("content-range", "")
        linked_size = headers.get("x-linked-size", "")
        reported_size = None
        if "/" in content_range:
            reported_size = int(content_range.rsplit("/", 1)[1])
        elif linked_size.isdigit():
            reported_size = int(linked_size)
        elif headers.get("content-length", "").isdigit() and response.status == 200:
            reported_size = int(headers["content-length"])
        if reported_size != size or header_digest(headers) != sha256:
            raise ValueError(f"remote large-file receipt mismatch: {url}")


def verify_remote_delivery(delivery: dict, packages: dict) -> None:
    packaged = {model["id"] for model in packages["models"]}
    for model in delivery["models"]:
        if model["id"] in packaged:
            archive = model["archive"]
            verify_remote(
                archive["url"], archive["sizeBytes"], archive["sha256"], True
            )
            print(f"remote {model['id']}: archive verified")
            continue
        for item in model["files"]:
            verify_remote(item["url"], item["sizeBytes"], item["sha256"], False)
        print(f"remote {model['id']}: {len(model['files'])} immutable files verified")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--package-manifest", type=Path, required=True)
    parser.add_argument(
        "--delivery-manifest", type=Path, default=Path("model-delivery/windows-v1.json")
    )
    parser.add_argument("--package-dir", type=Path)
    parser.add_argument("--remote", action="store_true")
    args = parser.parse_args()
    packages = json.loads(args.package_manifest.read_text(encoding="utf-8"))
    delivery = json.loads(args.delivery_manifest.read_text(encoding="utf-8"))
    expected = generate_delivery(copy.deepcopy(packages))
    if delivery != expected:
        raise ValueError("tracked delivery manifest is not the deterministic generated output")
    for model in delivery["models"]:
        validate_file_inventory(model)
        if "archive" in model:
            validate_url(model["archive"]["url"], model["archive"]["sha256"])
    package_dir = args.package_dir or args.package_manifest.parent
    verify_local_packages(packages, package_dir)
    if args.remote:
        verify_remote_delivery(delivery, packages)
    print(f"verified {len(delivery['models'])} Windows model deliveries")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, json.JSONDecodeError, zipfile.BadZipFile) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from error
