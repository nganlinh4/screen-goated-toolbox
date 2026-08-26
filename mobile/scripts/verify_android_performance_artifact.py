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
DEX_PREFIX = "classes"


def positive_limit(contract: dict[str, object], name: str) -> int:
    value = contract.get(name)
    if not isinstance(value, int) or value <= 0:
        raise ValueError(f"android.{name} must be a positive integer")
    return value


def source_profiles(contract_path: Path) -> tuple[Path, Path]:
    repo_root = contract_path.parent.parent
    profile_root = (
        repo_root
        / "mobile"
        / "androidApp"
        / "src"
        / "main"
        / "generated"
        / "baselineProfiles"
    )
    return profile_root / "baseline-prof.txt", profile_root / "startup-prof.txt"


def profile_rules(path: Path) -> set[str]:
    if not path.is_file():
        raise ValueError(f"generated profile does not exist: {path}")
    return {
        line.strip()
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    }


def verify(apk_path: Path, contract_path: Path) -> None:
    if not apk_path.is_file():
        raise ValueError(f"APK does not exist: {apk_path}")
    contract = json.loads(contract_path.read_text(encoding="utf-8"))
    android = contract.get("android")
    if not isinstance(android, dict):
        raise ValueError("performance contract has no android object")

    artifact_limit = positive_limit(android, "maxShippingArtifactBytes")
    profile_limit = positive_limit(android, "maxEmbeddedProfileBytes")
    dex_limit = positive_limit(android, "maxDexBytes")
    primary_dex_limit = positive_limit(android, "maxPrimaryDexBytes")
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
        dex_entries = [
            entry
            for entry in archive.infolist()
            if entry.filename.startswith(DEX_PREFIX) and entry.filename.endswith(".dex")
        ]
        if not dex_entries:
            raise ValueError("APK contains no classes*.dex entries")
        dex_size = sum(entry.file_size for entry in dex_entries)
        primary_dex = next(
            (entry for entry in dex_entries if entry.filename == "classes.dex"),
            None,
        )
        if primary_dex is None:
            raise ValueError("APK is missing classes.dex")

    if profile_size > profile_limit:
        raise ValueError(
            f"embedded profiles are {profile_size} bytes; contract allows {profile_limit}"
        )
    if dex_size > dex_limit:
        raise ValueError(f"DEX is {dex_size} bytes; contract allows {dex_limit}")
    if primary_dex.file_size > primary_dex_limit:
        raise ValueError(
            f"classes.dex is {primary_dex.file_size} bytes; "
            f"contract allows {primary_dex_limit}"
        )

    baseline_path, startup_path = source_profiles(contract_path)
    baseline = profile_rules(baseline_path)
    startup = profile_rules(startup_path)
    if not baseline or not startup:
        raise ValueError("generated Baseline and Startup Profiles must not be empty")
    profile_ratio = len(startup) / len(baseline)
    max_profile_ratio = android.get("maxStartupToBaselineRuleRatio")
    if not isinstance(max_profile_ratio, (int, float)) or not 0 < max_profile_ratio < 1:
        raise ValueError("android.maxStartupToBaselineRuleRatio must be between 0 and 1")
    if profile_ratio > max_profile_ratio:
        raise ValueError(
            f"Startup Profile contains {profile_ratio:.3%} of Baseline Profile rules; "
            f"contract allows {max_profile_ratio:.3%}"
        )
    overlap_ratio = len(startup & baseline) / len(startup)
    min_overlap_ratio = android.get("minStartupRuleOverlapRatio")
    if not isinstance(min_overlap_ratio, (int, float)) or not 0 < min_overlap_ratio <= 1:
        raise ValueError("android.minStartupRuleOverlapRatio must be between 0 and 1")
    if overlap_ratio < min_overlap_ratio:
        raise ValueError(
            f"Startup/Baseline rule overlap is {overlap_ratio:.3%}; "
            f"contract requires {min_overlap_ratio:.3%}"
        )
    print(
        f"Android performance artifact passed: apk={artifact_size} bytes, "
        f"dex={dex_size} bytes, classes.dex={primary_dex.file_size} bytes, "
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
