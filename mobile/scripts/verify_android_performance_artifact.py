#!/usr/bin/env python3
"""Verify release APK performance and size delivery invariants."""

from __future__ import annotations

import argparse
import json
import sys
import zipfile
from pathlib import Path


PROFILE_ENTRIES = (
    "assets/dexopt/baseline.prof",
    "assets/dexopt/baseline.profm",
)


def positive_limit(contract: dict[str, object], name: str) -> int:
    value = contract.get(name)
    if not isinstance(value, int) or value <= 0:
        raise ValueError(f"android.{name} must be a positive integer")
    return value


def verify(apk_path: Path, contract_path: Path) -> None:
    if not apk_path.is_file():
        raise ValueError(f"APK does not exist: {apk_path}")
    contract = json.loads(contract_path.read_text(encoding="utf-8"))
    android = contract.get("android")
    if not isinstance(android, dict):
        raise ValueError("performance contract has no android object")

    artifact_limit = positive_limit(android, "maxShippingArtifactBytes")
    profile_limit = positive_limit(android, "maxEmbeddedProfileBytes")
    artifact_size = apk_path.stat().st_size
    if artifact_size > artifact_limit:
        raise ValueError(
            f"APK is {artifact_size} bytes; contract allows {artifact_limit}"
        )

    with zipfile.ZipFile(apk_path) as archive:
        names = set(archive.namelist())
        missing = [entry for entry in PROFILE_ENTRIES if entry not in names]
        if missing:
            raise ValueError(f"APK is missing compiled profile entries: {missing}")
        profile_size = sum(archive.getinfo(entry).file_size for entry in PROFILE_ENTRIES)

    if profile_size > profile_limit:
        raise ValueError(
            f"embedded profiles are {profile_size} bytes; contract allows {profile_limit}"
        )
    print(
        f"Android performance artifact passed: apk={artifact_size} bytes, "
        f"profiles={profile_size} bytes"
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--apk", required=True, type=Path)
    parser.add_argument("--contract", required=True, type=Path)
    args = parser.parse_args()
    try:
        verify(args.apk.resolve(), args.contract.resolve())
    except (OSError, ValueError, json.JSONDecodeError, zipfile.BadZipFile) as error:
        print(f"Android performance artifact failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
