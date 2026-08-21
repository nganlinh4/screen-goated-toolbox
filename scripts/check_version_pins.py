#!/usr/bin/env python3
"""Report every version-pinned surface that disagrees with `Cargo.toml`.

`[package].version` is the source of truth, but several files repeat it and do
not follow automatically. Each one is asserted by a *different* build script, so
building to find them reports one stale pin and halts; the next is only
discovered after fixing the last. Worse, the Android build asserts
`creation-runtime-v1.json` too, so a pin missed during a bump can fail after the
desktop build has already passed.

This runs in a second without compiling anything, reports all of them together,
and with `--write` performs the bump itself.

    py -3 scripts/check_version_pins.py
    py -3 scripts/check_version_pins.py --write

`hostVersion` is only a host pin. Changing it does not invalidate the asset name,
`sha256`, or component `version` beside it, so a bump never requires
republishing a runtime bundle.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent


def crate_version() -> str:
    """The version every other surface must match."""
    text = (REPO / "Cargo.toml").read_text(encoding="utf-8")
    # Only the [package] table; a dependency pin further down is not ours.
    package = text.split("[package]", 1)[1].split("\n[", 1)[0]
    match = re.search(r'^version\s*=\s*"([^"]+)"', package, re.MULTILINE)
    if not match:
        raise SystemExit("Cargo.toml has no [package].version")
    return match.group(1)


def host_version_manifests() -> list[Path]:
    """Delivery manifests carrying a `hostVersion`.

    Discovered rather than listed, so a manifest added later is covered without
    anyone remembering to extend this script.
    """
    found = []
    for path in sorted((REPO / "component-delivery").rglob("*.json")):
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, UnicodeDecodeError):
            continue
        if isinstance(data, dict) and isinstance(data.get("hostVersion"), str):
            found.append(path)
    return found


def host_bound_cargo_manifests() -> list[Path]:
    """Cargo packages whose explicit metadata binds them to the host version."""
    excluded = {".git", "target", "node_modules", "third_party", "libs", "local-runtime-bundles"}
    found = []
    for path in sorted(REPO.rglob("Cargo.toml")):
        if excluded.intersection(path.relative_to(REPO).parts):
            continue
        try:
            package = tomllib.loads(path.read_text(encoding="utf-8")).get("package", {})
        except (tomllib.TOMLDecodeError, UnicodeDecodeError):
            continue
        metadata = package.get("metadata", {}).get("sgt", {})
        if metadata.get("host-version-bound") is True:
            found.append(path)
    return found


def app_rc_expectations(version: str) -> list[tuple[str, str]]:
    """The four spellings of the version in the Windows resource script.

    Two are comma-separated and two dotted; the installer shows whichever one is
    stale, so all four are checked.
    """
    dotted = f"{version}.0"
    comma = dotted.replace(".", ",")
    return [
        (rf"^FILEVERSION .*$", f"FILEVERSION {comma}"),
        (rf"^PRODUCTVERSION .*$", f"PRODUCTVERSION {comma}"),
        (r'VALUE "FileVersion", "[^"]*"', f'VALUE "FileVersion", "{dotted}"'),
        (r'VALUE "ProductVersion", "[^"]*"', f'VALUE "ProductVersion", "{dotted}"'),
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write",
        action="store_true",
        help="bump every stale pin to the Cargo.toml version instead of only reporting",
    )
    args = parser.parse_args()

    version = crate_version()
    stale: list[str] = []

    manifests = host_version_manifests()
    if not manifests:
        raise SystemExit("no component-delivery manifests carry a hostVersion")

    for path in manifests:
        raw = path.read_text(encoding="utf-8")
        pinned = json.loads(raw)["hostVersion"]
        if pinned == version:
            continue
        relative = path.relative_to(REPO).as_posix()
        stale.append(f"{relative}: hostVersion {pinned}")
        if args.write:
            # Edited textually so key order, indentation and trailing newline
            # survive; these manifests are reviewed as diffs.
            path.write_text(
                raw.replace(f'"hostVersion": "{pinned}"', f'"hostVersion": "{version}"'),
                encoding="utf-8",
                newline="",
            )

    for path in host_bound_cargo_manifests():
        raw = path.read_text(encoding="utf-8")
        package = tomllib.loads(raw)["package"]
        pinned = package["version"]
        if pinned == version:
            continue
        relative = path.relative_to(REPO).as_posix()
        stale.append(f"{relative}: package.version {pinned}")
        if args.write:
            path.write_text(
                raw.replace(f'version = "{pinned}"', f'version = "{version}"', 1),
                encoding="utf-8",
                newline="",
            )

    app_rc = REPO / "app.rc"
    raw = app_rc.read_text(encoding="utf-8")
    updated = raw
    for pattern, expected in app_rc_expectations(version):
        if re.search(re.escape(expected), updated):
            continue
        current = re.search(pattern, updated, re.MULTILINE)
        stale.append(f"app.rc: {current.group(0).strip() if current else pattern}")
        updated = re.sub(pattern, expected, updated, count=1, flags=re.MULTILINE)
    if args.write and updated != raw:
        app_rc.write_text(updated, encoding="utf-8", newline="")

    if not stale:
        print(f"every version-pinned surface matches Cargo.toml {version}")
        return 0

    verb = "bumped" if args.write else "disagree with"
    print(f"{verb} Cargo.toml {version}:", file=sys.stderr)
    for entry in stale:
        print(f"  {entry}", file=sys.stderr)
    if args.write:
        print("\nreview the diff before continuing", file=sys.stderr)
        return 0
    print("\nrerun with --write to bump them", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
