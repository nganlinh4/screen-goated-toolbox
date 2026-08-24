#!/usr/bin/env python3
"""Validate Android build inputs without serializing Gradle script closures."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


PRODUCTION_BUNDLE_PREFIX = (
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/"
    "download/sgt-runtime-bundles/sgt-downloader-"
)


def verify_viewer(root: Path) -> None:
    expected = {
        "creation_model_viewer/index.html",
        "creation_model_viewer/assets/viewer.css",
        "creation_model_viewer/assets/viewer.js",
    }
    if not root.is_dir():
        raise ValueError(f"Shared creation viewer build is missing: {root}")
    actual = {
        path.relative_to(root).as_posix()
        for path in root.rglob("*")
        if path.is_file()
    }
    if actual != expected:
        raise ValueError(
            f"Shared creation viewer must contain exactly {sorted(expected)}, "
            f"found {sorted(actual)}"
        )
    document = (root / "creation_model_viewer/index.html").read_text("utf-8")
    if 'data-viewer-version="2"' not in document:
        raise ValueError("Shared creation viewer document version is missing")
    if "default-src 'none'" not in document or "connect-src 'self'" not in document:
        raise ValueError("Shared creation viewer CSP must deny external resources")


def verify_component_key(source: Path) -> None:
    if not source.is_file():
        raise ValueError(f"Tracked component-update public key is required: {source}")
    if re.fullmatch(r"04[0-9a-f]{128}", source.read_text("utf-8").strip()) is None:
        raise ValueError("Component-update public key must be an uncompressed P-256 point")


def required_string(record: dict[str, object], name: str, context: str) -> str:
    value = record.get(name)
    if not isinstance(value, str) or not value:
        raise ValueError(f"{context} {name} is missing")
    return value


def verify_downloader_delivery(source: Path) -> None:
    if not source.is_file():
        raise ValueError(f"Full downloader delivery manifest is required: {source}")
    root = json.loads(source.read_text("utf-8"))
    if not isinstance(root, dict) or root.get("schemaVersion") != 1:
        raise ValueError("Unsupported downloader delivery schema")
    if root.get("abi") != "arm64-v8a":
        raise ValueError("Downloader delivery must target arm64-v8a")
    version = required_string(root, "version", "Downloader delivery")
    artifacts = root.get("artifacts")
    if not isinstance(artifacts, list):
        raise ValueError("Downloader delivery artifacts are missing")
    contracts: list[dict[str, object]] = []
    for record in artifacts:
        if not isinstance(record, dict):
            raise ValueError("Invalid downloader artifact")
        contracts.append(record)
    if len(contracts) != 3:
        raise ValueError("Downloader delivery repeats an artifact")
    roles = {record.get("role") for record in contracts}
    if roles != {"yt_dlp", "python", "ffmpeg"}:
        raise ValueError("Downloader delivery roles must be yt_dlp, python, and ffmpeg")

    for contract in contracts:
        role = required_string(contract, "role", "Downloader artifact")
        asset = required_string(contract, "asset", f"Downloader {role}")
        if "/" in asset or "\\" in asset:
            raise ValueError(f"Invalid downloader asset for {role}")
        url = required_string(contract, "downloadUrl", f"Downloader {role}")
        size = contract.get("sizeBytes")
        digest = required_string(contract, "sha256", f"Downloader {role}")
        if not isinstance(size, int) or size <= 0 or re.fullmatch(r"[0-9a-f]{64}", digest) is None:
            raise ValueError(f"Invalid downloader identity for {role}")
        if not url.endswith(f"/{asset}"):
            raise ValueError(f"Downloader asset URL differs for {role}")

        if role == "yt_dlp":
            match = re.fullmatch(
                r"https://github\.com/yt-dlp/yt-dlp/releases/download/([0-9.]+)/yt-dlp",
                url,
            )
            if match is None:
                raise ValueError("yt-dlp must use an immutable official release URL")
            if not version.startswith(match.group(1)):
                raise ValueError("yt-dlp version and delivery version differ")
            continue

        if not url.startswith(PRODUCTION_BUNDLE_PREFIX):
            raise ValueError(f"{role} must use a uniquely named sgt-runtime-bundles asset")
        if digest[:12] not in asset:
            raise ValueError(f"{role} asset must include its SHA-256 prefix")
        entry_count = contract.get("entryCount")
        unpacked_size = contract.get("uncompressedBytes")
        if (
            not isinstance(entry_count, int)
            or entry_count <= 0
            or not isinstance(unpacked_size, int)
            or unpacked_size <= 0
        ):
            raise ValueError(f"{role} extraction contract is incomplete")
        required_paths = contract.get("requiredPaths")
        if not isinstance(required_paths, list) or not required_paths:
            raise ValueError(f"{role} required paths are missing")


def verify_launchers(specifications: list[str]) -> None:
    for specification in specifications:
        path_text, size_text, expected_hash = specification.rsplit("|", 2)
        source = Path(path_text)
        if not source.is_file():
            raise ValueError(f"Full downloader launcher is missing: {source}")
        if source.stat().st_size != int(size_text):
            raise ValueError(f"Full downloader launcher size mismatch: {source}")
        actual_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        if actual_hash != expected_hash:
            raise ValueError(f"Full downloader launcher SHA-256 mismatch: {source}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    viewer = subparsers.add_parser("viewer")
    viewer.add_argument("--root", type=Path, required=True)
    component_key = subparsers.add_parser("component-key")
    component_key.add_argument("--file", type=Path, required=True)
    downloader = subparsers.add_parser("downloader-delivery")
    downloader.add_argument("--file", type=Path, required=True)
    launchers = subparsers.add_parser("launchers")
    launchers.add_argument("--launcher", action="append", default=[], required=True)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.command == "viewer":
        verify_viewer(args.root)
    elif args.command == "component-key":
        verify_component_key(args.file)
    elif args.command == "downloader-delivery":
        verify_downloader_delivery(args.file)
    elif args.command == "launchers":
        verify_launchers(args.launcher)


if __name__ == "__main__":
    main()
