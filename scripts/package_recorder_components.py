#!/usr/bin/env python3
"""Create the deterministic, independently removable Windows recorder bundle."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import struct
import subprocess
import sys
import zipfile

from dev_cache_paths import (
    managed_cache_root,
    manifest_path,
    require_repo_or_managed_cache,
)
from pathlib import Path


FIXED_TIMESTAMP = (1980, 1, 1, 0, 0, 0)
MACHINE_X64 = 0x8664
MAX_WEB_FILES = 512
LICENSE_PREFIXES = ("license", "licence", "copying", "notice", "copyright")
FIRST_PARTY_RUST_PACKAGES = {
    "sgt-language-catalog",
    "sgt-local-asr-protocol",
    "sgt-recorder-protocol",
    "sgt-recorder-worker",
}
STANDARD_LICENSE_MARKERS = {
    "MIT": b"Permission is hereby granted, free of charge",
    "Apache-2.0": b"Apache License\n                           Version 2.0",
    "MPL-2.0": b"Mozilla Public License Version 2.0",
    "BSL-1.0": b"Boost Software License - Version 1.0",
    "CC0-1.0": b"CC0 1.0 Universal",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def cargo_version(repo: Path) -> str:
    cargo = (repo / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', cargo, re.MULTILINE)
    if not match:
        raise RuntimeError("Cargo.toml package version is missing")
    return match.group(1)


def validate_x64_pe(path: Path) -> None:
    if not path.is_file() or path.is_symlink():
        raise RuntimeError(f"recorder worker is not a regular file: {path}")
    with path.open("rb") as stream:
        dos = stream.read(64)
        if len(dos) != 64 or dos[:2] != b"MZ":
            raise RuntimeError(f"recorder worker is not a PE executable: {path}")
        offset = struct.unpack_from("<I", dos, 0x3C)[0]
        stream.seek(offset)
        header = stream.read(6)
    if len(header) != 6 or header[:4] != b"PE\0\0":
        raise RuntimeError(f"recorder worker has an invalid PE header: {path}")
    if struct.unpack_from("<H", header, 4)[0] != MACHINE_X64:
        raise RuntimeError(f"recorder worker is not x64: {path}")


def license_files(package_root: Path) -> list[Path]:
    return sorted(
        path
        for path in package_root.iterdir()
        if path.is_file()
        and not path.is_symlink()
        and path.name.casefold().startswith(LICENSE_PREFIXES)
    )


def license_record(path: Path) -> dict[str, object]:
    data = path.read_bytes()
    return {
        "name": path.name,
        "sizeBytes": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
        "base64": base64.b64encode(data).decode("ascii"),
    }


def rust_license_inventory(repo: Path, manifest: Path) -> list[dict[str, object]]:
    result = subprocess.run(
        [
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--filter-platform",
            "x86_64-pc-windows-msvc",
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
    package_by_id = {package["id"]: package for package in metadata["packages"]}
    node_by_id = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    reachable = set()
    pending = [metadata["resolve"]["root"]]
    while pending:
        package_id = pending.pop()
        if package_id in reachable:
            continue
        reachable.add(package_id)
        pending.extend(dependency["pkg"] for dependency in node_by_id[package_id]["deps"])

    packages = []
    for package_id in sorted(reachable):
        package = package_by_id[package_id]
        if package["name"] in FIRST_PARTY_RUST_PACKAGES:
            continue
        package_root = Path(package["manifest_path"]).parent
        files = [license_record(path) for path in license_files(package_root)]
        packages.append(
            {
                "name": package["name"],
                "version": package["version"],
                "licenseExpression": package.get("license"),
                "licenseFile": package.get("license_file"),
                "repository": package.get("repository"),
                "files": files,
            }
        )
    return sorted(packages, key=lambda item: (item["name"], item["version"]))


def web_license_inventory(web_project: Path) -> list[dict[str, object]]:
    lock = json.loads((web_project / "package-lock.json").read_text(encoding="utf-8"))
    packages = []
    for relative, package in sorted(lock.get("packages", {}).items()):
        if not relative or "node_modules/" not in relative:
            continue
        if package.get("dev"):
            continue
        package_root = web_project / relative
        if not package_root.is_dir():
            if package.get("optional"):
                continue
            raise RuntimeError(f"npm package directory is missing: {relative}")
        package_json = package_root / "package.json"
        package_data = (
            json.loads(package_json.read_text(encoding="utf-8"))
            if package_json.is_file()
            else {}
        )
        files = [license_record(path) for path in license_files(package_root)]
        packages.append(
            {
                "name": package.get("name") or package_data.get("name") or relative,
                "version": package.get("version") or package_data.get("version"),
                "licenseExpression": package.get("license") or package_data.get("license"),
                "repository": package_data.get("repository"),
                "files": files,
            }
        )
    return packages


def license_ids(expression: str | None) -> list[str]:
    if not expression:
        return []
    return [
        license_id
        for license_id in STANDARD_LICENSE_MARKERS
        if re.search(rf"(?<![A-Za-z0-9.-]){re.escape(license_id)}(?![A-Za-z0-9.-])", expression)
    ]


def complete_license_inventory(kind: str, packages: list[dict[str, object]]) -> dict[str, object]:
    required = set()
    for package in packages:
        if package["files"]:
            continue
        ids = license_ids(package.get("licenseExpression"))
        if not ids:
            raise RuntimeError(
                f"{kind} package has no distributable license text: "
                f"{package['name']} {package['version']}"
            )
        package["licenseTextIds"] = ids
        required.update(ids)

    corpus = {}
    for license_id in sorted(required):
        marker = STANDARD_LICENSE_MARKERS[license_id]
        match = None
        for package in packages:
            for record in package["files"]:
                if marker in base64.b64decode(record["base64"]):
                    match = record
                    break
            if match:
                break
        if not match:
            raise RuntimeError(f"complete {license_id} text is missing from {kind} inventory")
        corpus[license_id] = match
    return {
        "schemaVersion": 1,
        "kind": kind,
        "packages": packages,
        "standardLicenseTexts": corpus,
    }


def write_license_inventories(
    repo: Path, worker_manifest: Path, output: Path
) -> tuple[Path, Path]:
    targets = []
    inventories = [
        complete_license_inventory(
            "recorder-worker", rust_license_inventory(repo, worker_manifest)
        ),
        complete_license_inventory(
            "recorder-web", web_license_inventory(repo / "screen-record")
        ),
    ]
    for inventory in inventories:
        target = output / f".{inventory['kind']}-third-party-licenses.json"
        target.write_text(
            json.dumps(inventory, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
            + "\n",
            encoding="utf-8",
        )
        targets.append(target)
    return targets[0], targets[1]


def deterministic_zip(target: Path, files: list[tuple[str, Path]]) -> None:
    with zipfile.ZipFile(
        target, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for relative, source in sorted(files):
            info = zipfile.ZipInfo(relative, FIXED_TIMESTAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            with source.open("rb") as reader, archive.open(info, "w") as writer:
                for chunk in iter(lambda: reader.read(1024 * 1024), b""):
                    writer.write(chunk)


def package(
    repo: Path,
    output: Path,
    component_id: str,
    version: str,
    files: list[tuple[str, Path]],
) -> dict[str, object]:
    if not files or len(files) > MAX_WEB_FILES:
        raise RuntimeError(f"{component_id} has an invalid file count")
    paths = [relative for relative, _ in files]
    if paths != sorted(paths) or len(set(paths)) != len(paths):
        raise RuntimeError(f"{component_id} inventory is not sorted and unique")
    for relative, source in files:
        parts = Path(relative).parts
        if (
            not relative
            or "\\" in relative
            or Path(relative).is_absolute()
            or any(part in ("", ".", "..") for part in parts)
            or not source.is_file()
            or source.is_symlink()
        ):
            raise RuntimeError(f"unsafe recorder package entry: {relative}")

    first = output / f".{component_id}.first.zip"
    second = output / f".{component_id}.second.zip"
    first.unlink(missing_ok=True)
    second.unlink(missing_ok=True)
    deterministic_zip(first, files)
    deterministic_zip(second, files)
    if sha256(first) != sha256(second):
        raise RuntimeError(f"nondeterministic recorder archive: {component_id}")
    second.unlink()

    archive_hash = sha256(first)
    asset = f"{component_id}-{version}-{archive_hash[:16]}.zip"
    target = output / asset
    if target.exists() and sha256(target) != archive_hash:
        raise RuntimeError(f"refusing to replace immutable asset: {target}")
    if target.exists():
        first.unlink()
    else:
        first.replace(target)

    records = [
        {
            "path": relative,
            "sizeBytes": source.stat().st_size,
            "sha256": sha256(source),
        }
        for relative, source in files
    ]
    return {
        "id": component_id,
        "version": version,
        "asset": asset,
        "assetPath": manifest_path(repo, target),
        "sizeBytes": target.stat().st_size,
        "sha256": archive_hash,
        "unpackedSizeBytes": sum(record["sizeBytes"] for record in records),
        "files": records,
    }


def require_matching_delivery(output: Path, descriptor: dict[str, object]) -> None:
    path = output / "sgt_recorder.delivery.json"
    if not path.is_file():
        raise RuntimeError(
            "verified recorder delivery is missing; upload the immutable packages and "
            "run verify_recorder_release.py"
        )
    delivery = json.loads(path.read_text(encoding="utf-8"))
    expected = descriptor["components"]
    actual = delivery.get("components", [])
    keys = (
        "id",
        "version",
        "asset",
        "sizeBytes",
        "sha256",
        "unpackedSizeBytes",
        "files",
    )
    expected_values = [{key: item[key] for key in keys} for item in expected]
    actual_values = [{key: item.get(key) for key in keys} for item in actual]
    if (
        delivery.get("schemaVersion") != 1
        or delivery.get("architecture") != "x64"
        or actual_values != expected_values
    ):
        raise RuntimeError("verified recorder delivery does not match current packages")


def reuse_verified_asset_names(
    descriptor: dict[str, object], delivery: dict[str, object]
) -> None:
    """Keep immutable recorder asset names when their packaged bytes are unchanged."""
    delivered_by_id = {
        item.get("id"): item for item in delivery.get("components", [])
    }
    payload_fields = ("sizeBytes", "sha256", "unpackedSizeBytes", "files")
    for component in descriptor["components"]:
        delivered = delivered_by_id.get(component["id"])
        if delivered is None or not delivered.get("asset"):
            continue
        if all(component[field] == delivered.get(field) for field in payload_fields):
            component["asset"] = delivered["asset"]


def main() -> int:
    cache_root = managed_cache_root()
    parser = argparse.ArgumentParser()
    parser.add_argument("--version")
    parser.add_argument(
        "--worker-exe",
        default=str(
            cache_root
            / "cargo"
            / "package"
            / "x86_64-pc-windows-msvc"
            / "release"
            / "sgt-recorder-worker.exe"
        ),
    )
    parser.add_argument(
        "--web-root", default="screen-record/dist"
    )
    parser.add_argument(
        "--notice",
        default="component-notices/recorder-worker/THIRD-PARTY-NOTICES.txt",
    )
    parser.add_argument(
        "--web-notice",
        default="component-notices/recorder-web/THIRD-PARTY-NOTICES.txt",
    )
    parser.add_argument(
        "--worker-manifest", default="native/recorder_worker/Cargo.toml"
    )
    parser.add_argument(
        "--output-dir",
        default=str(cache_root / "packages" / "jobs" / "recorder"),
    )
    parser.add_argument("--require-delivery", action="store_true")
    args = parser.parse_args()

    repo = Path(__file__).resolve().parents[1]
    version = args.version or cargo_version(repo)
    if not re.fullmatch(r"[a-z0-9._-]{1,80}", version):
        raise RuntimeError("invalid recorder component version")
    output = require_repo_or_managed_cache(
        repo, repo / args.output_dir, "recorder output"
    )
    output.mkdir(parents=True, exist_ok=True)
    web_root = (repo / args.web_root).resolve()
    web_root.relative_to(repo)
    worker = require_repo_or_managed_cache(
        repo, repo / args.worker_exe, "recorder worker"
    )
    notice = (repo / args.notice).resolve()
    web_notice = (repo / args.web_notice).resolve()
    worker_manifest = (repo / args.worker_manifest).resolve()
    validate_x64_pe(worker)
    worker_licenses, web_licenses = write_license_inventories(
        repo, worker_manifest, output
    )

    web_files = sorted(
        (path.relative_to(web_root).as_posix(), path)
        for path in web_root.rglob("*")
        if path.is_file()
    )
    web_files.extend(
        [
            ("licenses/THIRD-PARTY-LICENSES.json", web_licenses),
            ("licenses/THIRD-PARTY-NOTICES.txt", web_notice),
        ]
    )
    web_files.sort()
    required = {"index.html", "assets/index.js", "assets/index.css"}
    if not required.issubset({relative for relative, _ in web_files}):
        raise RuntimeError("recorder web build is missing required entry files")
    bundle_files = [(f"web/{relative}", source) for relative, source in web_files]
    bundle_files.extend(
        [
            ("bin/x64/sgt-recorder-worker.exe", worker),
            ("licenses/worker/THIRD-PARTY-LICENSES.json", worker_licenses),
            ("licenses/worker/THIRD-PARTY-NOTICES.txt", notice),
        ]
    )
    bundle_files.sort()
    components = [package(repo, output, "screen-recorder", version, bundle_files)]
    descriptor: dict[str, object] = {
        "schemaVersion": 1,
        "architecture": "x64",
        "components": components,
    }
    delivery_path = output / "sgt_recorder.delivery.json"
    if delivery_path.is_file():
        reuse_verified_asset_names(
            descriptor,
            json.loads(delivery_path.read_text(encoding="utf-8")),
        )
    path = output / "sgt_recorder.packages.json"
    path.write_text(
        json.dumps(descriptor, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    if args.require_delivery:
        require_matching_delivery(output, descriptor)
    worker_licenses.unlink()
    web_licenses.unlink()
    print(path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, OSError, RuntimeError, ValueError, zipfile.BadZipFile) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
