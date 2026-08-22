#!/usr/bin/env python3
"""Build the signed stable app-update manifest from a published installer."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path

from ecdsa.util import sigencode_string

from build_update_catalog import PUBLIC_KEY, private_key, public_hex


VERSION_PATTERN = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+")
REPOSITORY = "nganlinh4/screen-goated-toolbox"


def build_payload(version: str, installer: Path, release_notes: str) -> bytes:
    if not VERSION_PATTERN.fullmatch(version):
        raise SystemExit("stable app version must be numeric major.minor.patch")
    expected_name = f"ScreenGoatedToolbox_v{version}.exe"
    if installer.name != expected_name or not installer.is_file():
        raise SystemExit(f"installer must be an existing {expected_name}")
    data = installer.read_bytes()
    payload = {
        "schemaVersion": 1,
        "channel": "stable",
        "version": version,
        "releaseNotes": release_notes,
        "installer": {
            "name": expected_name,
            "url": (
                f"https://github.com/{REPOSITORY}/releases/download/"
                f"v{version}/{expected_name}"
            ),
            "sizeBytes": len(data),
            "sha256": hashlib.sha256(data).hexdigest(),
        },
    }
    encoded = (json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + "\n").encode()
    if len(encoded) > 64 * 1024:
        raise SystemExit("app update manifest exceeds the client size limit")
    return encoded


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--installer", type=Path, required=True)
    parser.add_argument("--release-notes", type=Path, required=True)
    parser.add_argument("--private-key", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    key = private_key(args)
    expected_public = PUBLIC_KEY.read_text(encoding="ascii").strip().lower()
    if public_hex(key) != expected_public:
        raise SystemExit("signing key does not match the tracked update public key")

    payload = build_payload(
        args.version,
        args.installer,
        args.release_notes.read_text(encoding="utf-8"),
    )
    signature = key.sign_digest_deterministic(
        hashlib.sha256(payload).digest(),
        hashfunc=hashlib.sha256,
        sigencode=sigencode_string,
    )
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "stable-v1.json").write_bytes(payload)
    (args.output / "stable-v1.sig").write_bytes(signature)
    print(hashlib.sha256(payload).hexdigest())


if __name__ == "__main__":
    main()
