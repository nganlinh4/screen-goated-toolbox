#!/usr/bin/env python3
"""Create the deterministic, independently removable Computer Control engine."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import struct
import subprocess
import sys
import zipfile

from dev_cache_paths import manifest_path, require_repo_or_managed_cache
from pathlib import Path


FIXED_TIMESTAMP = (1980, 1, 1, 0, 0, 0)
MACHINE_X64 = 0x8664
COMPONENT_ID = "computer-control-engine"
RELEASE_PREFIX = (
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/"
    "sgt-runtime-bundles/"
)
LICENSE_PREFIXES = ("license", "licence", "copying", "notice", "copyright")
FIRST_PARTY = {"sgt-computer-control-engine", "sgt-computer-control-protocol"}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def worker_version(manifest: Path) -> str:
    match = re.search(
        r'^version\s*=\s*"([^"]+)"',
        manifest.read_text(encoding="utf-8"),
        re.MULTILINE,
    )
    if not match:
        raise RuntimeError("Computer Control engine version is missing")
    return match.group(1)


def validate_x64_pe(path: Path) -> None:
    if not path.is_file() or path.is_symlink():
        raise RuntimeError(f"engine is not a regular file: {path}")
    with path.open("rb") as stream:
        dos = stream.read(64)
        if len(dos) != 64 or dos[:2] != b"MZ":
            raise RuntimeError(f"engine is not a PE executable: {path}")
        offset = struct.unpack_from("<I", dos, 0x3C)[0]
        stream.seek(offset)
        header = stream.read(6)
    if header[:4] != b"PE\0\0" or struct.unpack_from("<H", header, 4)[0] != MACHINE_X64:
        raise RuntimeError("Computer Control engine is not an x64 PE executable")


def reject_private_paths(path: Path, repo: Path) -> None:
    content = path.read_bytes().lower()
    candidates = [repo, Path.home()]
    cargo_home = os.environ.get("CARGO_HOME")
    if cargo_home:
        candidates.append(Path(cargo_home))
    for candidate in candidates:
        for spelling in {str(candidate), str(candidate).replace("\\", "/")}:
            if spelling.encode("utf-8").lower() in content:
                raise RuntimeError(f"engine contains a private build path: {spelling}")


def license_files(package_root: Path) -> list[Path]:
    return sorted(
        path
        for path in package_root.iterdir()
        if path.is_file()
        and not path.is_symlink()
        and path.name.casefold().startswith(LICENSE_PREFIXES)
    )


def dependency_packages(repo: Path, manifest: Path) -> list[dict[str, object]]:
    result = subprocess.run(
        [
            "cargo",
            "metadata",
            "--locked",
            "--filter-platform",
            "x86_64-pc-windows-msvc",
            "--format-version",
            "1",
            "--manifest-path",
            str(manifest),
        ],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    metadata = json.loads(result.stdout)
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    pending = [metadata["resolve"]["root"]]
    reachable: set[str] = set()
    while pending:
        package_id = pending.pop()
        if package_id in reachable:
            continue
        reachable.add(package_id)
        pending.extend(dependency["pkg"] for dependency in nodes[package_id]["deps"])
    return [
        packages[package_id]
        for package_id in reachable
        if packages[package_id]["name"] not in FIRST_PARTY
    ]


def write_licenses(repo: Path, manifest: Path, output: Path) -> tuple[Path, Path]:
    records = []
    notice_sections = [
        "Screen Goated Toolbox - Computer Control engine third-party notices",
        "Full upstream license files follow for every resolved third-party package.",
    ]
    for package in sorted(
        dependency_packages(repo, manifest), key=lambda item: (item["name"], item["version"])
    ):
        package_root = Path(package["manifest_path"]).parent
        files = license_files(package_root)
        if not files:
            raise RuntimeError(f"license files are missing for {package['name']}")
        file_records = []
        notice_sections.append(
            f"\n{'=' * 78}\n{package['name']} {package['version']}\n"
            f"License: {package.get('license')}\nSource: {package.get('repository')}"
        )
        for path in files:
            data = path.read_bytes()
            text = data.decode("utf-8")
            file_records.append(
                {
                    "name": path.name,
                    "sizeBytes": len(data),
                    "sha256": hashlib.sha256(data).hexdigest(),
                    "base64": base64.b64encode(data).decode("ascii"),
                }
            )
            notice_sections.append(f"\n--- {path.name} ---\n{text.rstrip()}\n")
        records.append(
            {
                "name": package["name"],
                "version": package["version"],
                "licenseExpression": package.get("license"),
                "repository": package.get("repository"),
                "files": file_records,
            }
        )
    inventory = output / ".computer-control-third-party-licenses.json"
    notices = output / ".computer-control-third-party-notices.txt"
    inventory.write_text(
        json.dumps(
            {"schemaVersion": 1, "rust": records},
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
        )
        + "\n",
        encoding="utf-8",
    )
    notices.write_text("\n".join(notice_sections), encoding="utf-8")
    return inventory, notices


def deterministic_zip(target: Path, files: list[tuple[str, Path]]) -> None:
    with zipfile.ZipFile(
        target, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for relative, source in sorted(files):
            info = zipfile.ZipInfo(relative, FIXED_TIMESTAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            mode = 0o100755 if relative.endswith(".exe") else 0o100644
            info.external_attr = mode << 16
            with source.open("rb") as reader, archive.open(info, "w") as writer:
                for chunk in iter(lambda: reader.read(1024 * 1024), b""):
                    writer.write(chunk)


def package(repo: Path, output: Path, executable: Path, manifest: Path) -> dict[str, object]:
    validate_x64_pe(executable)
    reject_private_paths(executable, repo)
    inventory, notices = write_licenses(repo, manifest, output)
    files = sorted(
        [
            ("bin/x64/sgt-computer-control-engine.exe", executable),
            ("licenses/THIRD-PARTY-LICENSES.json", inventory),
            ("licenses/THIRD-PARTY-NOTICES.txt", notices),
        ]
    )
    first = output / ".computer-control.first.zip"
    second = output / ".computer-control.second.zip"
    first.unlink(missing_ok=True)
    second.unlink(missing_ok=True)
    deterministic_zip(first, files)
    deterministic_zip(second, files)
    if sha256(first) != sha256(second):
        raise RuntimeError("Computer Control engine archive is nondeterministic")
    second.unlink()
    version = worker_version(manifest)
    archive_hash = sha256(first)
    asset = f"{COMPONENT_ID}-{version}-{archive_hash[:16]}.zip"
    target = output / asset
    if target.exists() and sha256(target) != archive_hash:
        raise RuntimeError(f"refusing to replace immutable asset: {target}")
    if target.exists():
        first.unlink()
    else:
        first.replace(target)
    records = [
        {"path": relative, "sizeBytes": source.stat().st_size, "sha256": sha256(source)}
        for relative, source in files
    ]
    return {
        "id": COMPONENT_ID,
        "version": version,
        "asset": asset,
        "assetPath": manifest_path(repo, target),
        "sizeBytes": target.stat().st_size,
        "sha256": archive_hash,
        "unpackedSizeBytes": sum(record["sizeBytes"] for record in records),
        "files": records,
    }


def require_delivery(output: Path, component: dict[str, object]) -> None:
    delivery_path = output / "sgt_computer_control.delivery.json"
    if not delivery_path.is_file():
        raise RuntimeError("verified Computer Control delivery is missing")
    delivery = json.loads(delivery_path.read_text(encoding="utf-8"))
    delivered = delivery.get("component", {})
    keys = ("id", "version", "asset", "sizeBytes", "sha256", "unpackedSizeBytes", "files")
    if (
        delivery.get("schemaVersion") != 1
        or delivery.get("architecture") != "x64"
        or {key: delivered.get(key) for key in keys}
        != {key: component[key] for key in keys}
        or delivered.get("downloadUrl") != RELEASE_PREFIX + component["asset"]
    ):
        raise RuntimeError("verified Computer Control delivery does not match current package")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--worker-exe", required=True)
    parser.add_argument("--output-dir", default="local-runtime-bundles/sgt_computer_control")
    parser.add_argument("--require-delivery", action="store_true")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    output = require_repo_or_managed_cache(
        repo, repo / args.output_dir, "Computer Control output"
    )
    executable = require_repo_or_managed_cache(
        repo, repo / args.worker_exe, "Computer Control worker"
    )
    output.mkdir(parents=True, exist_ok=True)
    manifest = repo / "native/computer_control_engine/Cargo.toml"
    component = package(repo, output, executable, manifest)
    descriptor = {"schemaVersion": 1, "architecture": "x64", "component": component}
    packages_path = output / "sgt_computer_control.packages.json"
    packages_path.write_text(
        json.dumps(descriptor, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    if args.require_delivery:
        require_delivery(output, component)
    print(packages_path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
