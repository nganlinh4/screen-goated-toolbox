#!/usr/bin/env python3
"""Manage mutable development candidates and immutable production components."""

from __future__ import annotations

import argparse
import copy
from pathlib import Path
import subprocess
import sys
import tempfile
from typing import Any, Iterator

from component_release_github import (
    download_asset,
    ensure_staging_release,
    gh,
    release_assets,
    run,
    sha256,
    verify_remote,
)
from component_release_paths import (
    cache_root,
    read_json,
    remove_local_staged_contract,
    safe_contract_relative,
    write_json,
)

REPOSITORY = "nganlinh4/screen-goated-toolbox"
STAGING_TAG = "sgt-runtime-staging"
PRODUCTION_TAG = "sgt-runtime-bundles"
INDEX_ASSET = "sgt-staging-index.json"
MAX_GITHUB_ASSET = 2_000_000_000
PREFIX = "https://github.com/{repo}/releases/download/{tag}/{asset}"
LOCAL_ONLY_FIELDS = {"assetPath", "sourceUrl", "sources"}
ANDROID_CREATION_SELECTORS = frozenset({"android/full", "android/play"})
ANDROID_SHARED_FIELDS = ("factoryClass", "entries")


def validate_digest(value: Any, label: str) -> str:
    if not isinstance(value, str) or len(value) != 64:
        raise ValueError(f"{label} has an invalid SHA-256")
    if any(character not in "0123456789abcdef" for character in value):
        raise ValueError(f"{label} has a non-lowercase SHA-256")
    return value


def iter_id_nodes(value: Any) -> Iterator[dict[str, Any]]:
    if isinstance(value, dict):
        if isinstance(value.get("id"), str):
            yield value
        for child in value.values():
            yield from iter_id_nodes(child)
    elif isinstance(value, list):
        for child in value:
            yield from iter_id_nodes(child)


def clean_copy(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: clean_copy(child)
            for key, child in value.items()
            if key not in LOCAL_ONLY_FIELDS
        }
    if isinstance(value, list):
        return [clean_copy(child) for child in value]
    return copy.deepcopy(value)


def parse_selectors(values: list[str] | None) -> set[str]:
    selected: set[str] = set()
    for value in values or []:
        parts = value.split("/")
        invalid_part = any(not part or part in {".", ".."} for part in parts)
        if not value or "\\" in value or invalid_part or value in selected:
            raise ValueError(f"invalid or repeated component selector: {value!r}")
        selected.add(value)
    return selected


def merge_android_creation_candidate(base: dict[str, Any], package: dict[str, Any], selected: set[str]) -> dict[str, Any]:
    if selected != ANDROID_CREATION_SELECTORS:
        raise ValueError("android/full and android/play must be selected together")
    base_android = base.get("android")
    source_android = package.get("android")
    if not isinstance(base_android, dict) or not isinstance(source_android, dict):
        raise ValueError("Android creation runtime records must be objects")
    for key in ("schemaVersion", "hostVersion", "version", "features"):
        if key in base and key in package and base[key] != package[key]:
            raise ValueError(f"Android package has incompatible {key}")
    for selector in sorted(ANDROID_CREATION_SELECTORS):
        key = selector.rsplit("/", 1)[-1]
        record = source_android.get(key)
        if not isinstance(record, dict) or not asset_name(record):
            raise ValueError(f"package manifest is missing {selector}")
        size = record.get("sizeBytes")
        if not isinstance(size, int) or isinstance(size, bool) or size <= 0:
            raise ValueError(f"{selector} has an invalid size")
        validate_digest(record.get("sha256"), selector)
    factory = source_android.get("factoryClass")
    entries = source_android.get("entries")
    if not isinstance(factory, str) or not factory:
        raise ValueError("package manifest is missing Android factoryClass")
    if not isinstance(entries, list) or not entries or not all(isinstance(entry, dict) for entry in entries):
        raise ValueError("package manifest has invalid Android entries")
    for index, entry in enumerate(entries):
        size = entry.get("sizeBytes")
        required = ("archivePath", "installPath", "role")
        if not isinstance(size, int) or isinstance(size, bool) or size <= 0 or any(
            not isinstance(entry.get(key), str) or not entry[key] for key in required
        ):
            raise ValueError(f"Android entry {index} has invalid metadata")
        validate_digest(entry.get("sha256"), f"Android entry {index}")
    candidate = copy.deepcopy(base)
    target_android = candidate["android"]
    for selector in sorted(ANDROID_CREATION_SELECTORS):
        key = selector.rsplit("/", 1)[-1]
        target_android[key] = clean_copy(source_android[key])
    for key in ANDROID_SHARED_FIELDS:
        target_android[key] = clean_copy(source_android[key])
    return candidate


def merge_candidate(base: dict[str, Any], package: dict[str, Any], selected: set[str]) -> dict[str, Any]:
    android_selected = {
        selector
        for selector in selected
        if selector == "android" or selector.startswith("android/")
    }
    if android_selected:
        if android_selected != ANDROID_CREATION_SELECTORS:
            raise ValueError("android/full and android/play must be selected together")
        other_selected = selected - ANDROID_CREATION_SELECTORS
        candidate = (
            merge_candidate(base, package, other_selected)
            if other_selected
            else copy.deepcopy(base)
        )
        return merge_android_creation_candidate(
            candidate,
            package,
            set(ANDROID_CREATION_SELECTORS),
        )
    candidate = copy.deepcopy(base)
    targets = {node["id"]: node for node in iter_id_nodes(candidate)}
    for source in iter_id_nodes(package):
        identifier = source["id"]
        if selected and identifier not in selected:
            continue
        target = targets.get(identifier)
        if target is not None:
            target.update(clean_copy(source))

    for key in ("schemaVersion", "version", "hostVersion", "architecture"):
        if key in package and key in candidate:
            candidate[key] = copy.deepcopy(package[key])

    for key, source in package.items():
        if (
            isinstance(source, dict)
            and "asset" in source
            and isinstance(candidate.get(key), dict)
            and (not selected or key in selected)
        ):
            candidate[key].update(clean_copy(source))
    return candidate


def asset_name(record: dict[str, Any]) -> str | None:
    name = record.get("asset")
    if isinstance(name, str) and name and "/" not in name and "\\" not in name:
        return name
    url = record.get("url")
    if isinstance(url, str) and "/" in url:
        name = url.rsplit("/", 1)[-1]
        if name and "/" not in name and "\\" not in name:
            return name
    return None


def iter_asset_records(
    value: Any,
    path: tuple[str, ...] = (),
    owner: str | None = None,
) -> Iterator[tuple[str, dict[str, Any]]]:
    if isinstance(value, dict):
        current_owner = value.get("id") if isinstance(value.get("id"), str) else owner
        name = asset_name(value)
        if name and isinstance(value.get("sizeBytes"), int) and "sha256" in value:
            key = current_owner or "/".join(path) or name
            yield key, value
        for key, child in value.items():
            yield from iter_asset_records(child, (*path, key), current_owner)
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from iter_asset_records(child, (*path, str(index)), owner)


def resolve_local_asset(
    repo: Path, asset_root: Path, record: dict[str, Any]
) -> Path | None:
    explicit = record.get("assetPath")
    if isinstance(explicit, str) and explicit:
        path = Path(explicit)
        if not path.is_absolute():
            path = repo / path
        return path.resolve()
    name = asset_name(record)
    return (asset_root / name).resolve() if name else None


def collect_local_assets(
    repo: Path,
    package: dict[str, Any],
    asset_root: Path,
    selected: set[str],
) -> list[dict[str, Any]]:
    assets: list[dict[str, Any]] = []
    seen: set[str] = set()
    for owner, record in iter_asset_records(package):
        if selected and owner not in selected:
            continue
        path = resolve_local_asset(repo, asset_root, record)
        if not path or not path.is_file() or path.is_symlink():
            continue
        name = asset_name(record)
        if not name or name in seen:
            raise ValueError(f"invalid or repeated local asset {name!r}")
        size = record["sizeBytes"]
        digest = validate_digest(record["sha256"], name)
        if digest[:16] not in name:
            raise ValueError(f"local staging asset is not content-addressed: {name}")
        if size <= 0 or size > MAX_GITHUB_ASSET:
            raise ValueError(f"{name} exceeds the supported GitHub asset boundary")
        if path.stat().st_size != size or sha256(path) != digest:
            raise ValueError(f"local package bytes differ from manifest: {path}")
        seen.add(name)
        assets.append(
            {
                "owner": owner,
                "asset": name,
                "path": str(path),
                "sizeBytes": size,
                "sha256": digest,
            }
        )
    if not assets:
        raise ValueError("no selected local component assets were found")
    return assets


def validate_asset_selection(assets: list[dict[str, Any]], selected: set[str]) -> None:
    if not assets or not all(isinstance(asset, dict) for asset in assets):
        raise ValueError("staging candidate has no valid assets")
    owners = {asset.get("owner") for asset in assets}
    if selected and owners != selected:
        raise ValueError("staging assets do not exactly match selected components")
    names: set[str] = set()
    for asset in assets:
        name = asset.get("asset")
        size = asset.get("sizeBytes")
        if (
            not isinstance(name, str)
            or asset_name(asset) != name
            or name in names
            or not isinstance(size, int)
            or isinstance(size, bool)
            or size <= 0
            or size > MAX_GITHUB_ASSET
        ):
            raise ValueError("staging candidate has invalid asset metadata")
        digest = validate_digest(asset.get("sha256"), name)
        if digest[:16] not in name:
            raise ValueError(f"staging asset is not content-addressed: {name}")
        names.add(name)


def validate_preserved_assets(base: dict[str, Any], names: set[str], selected: set[str]) -> None:
    if not ANDROID_CREATION_SELECTORS.issubset(selected) or "windows" in selected:
        return
    windows = base.get("windows")
    if not isinstance(windows, dict) or asset_name(windows) in names:
        raise ValueError("selected Android assets conflict with the tracked Windows record")


def rewrite_urls(value: Any, names: set[str], tag: str, repository: str) -> None:
    if isinstance(value, dict):
        name = asset_name(value)
        if name in names:
            url = PREFIX.format(repo=repository, tag=tag, asset=name)
            if "url" in value and "asset" not in value:
                value["url"] = url
            else:
                value["downloadUrl"] = url
        value.pop("assetPath", None)
        value.pop("sourceUrl", None)
        for child in value.values():
            rewrite_urls(child, names, tag, repository)
    elif isinstance(value, list):
        for child in value:
            rewrite_urls(child, names, tag, repository)


def empty_index(repository: str) -> dict[str, Any]:
    return {
        "schemaVersion": 1,
        "repository": repository,
        "tag": STAGING_TAG,
        "contracts": {},
    }


def load_staging_index(repository: str) -> dict[str, Any]:
    try:
        assets = release_assets(repository, STAGING_TAG)
    except subprocess.CalledProcessError:
        return empty_index(repository)
    if INDEX_ASSET not in assets:
        return empty_index(repository)
    with tempfile.TemporaryDirectory(prefix="sgt-staging-index-") as directory:
        path = Path(directory, INDEX_ASSET)
        download_asset(repository, STAGING_TAG, INDEX_ASSET, path)
        index = read_json(path)
    if (
        index.get("schemaVersion") != 1
        or index.get("repository") != repository
        or index.get("tag") != STAGING_TAG
        or not isinstance(index.get("contracts"), dict)
    ):
        raise ValueError("remote staging index has an invalid contract")
    return index


def upload_missing(repository: str, tag: str, asset: dict[str, Any]) -> None:
    existing = release_assets(repository, tag).get(asset["asset"])
    if existing:
        verify_remote(
            repository,
            tag,
            asset["asset"],
            asset["sizeBytes"],
            asset["sha256"],
        )
        return
    gh("release", "upload", tag, asset["path"], "--repo", repository)
    verify_remote(
        repository,
        tag,
        asset["asset"],
        asset["sizeBytes"],
        asset["sha256"],
    )


def stage(args: argparse.Namespace) -> int:
    repository = args.repository
    package_path = Path(args.package_manifest).resolve()
    tracked_path = Path(args.tracked_manifest).resolve()
    package = read_json(package_path)
    relative = safe_contract_relative(args.contract_relative)
    base = read_json(tracked_path)
    selected = parse_selectors(args.select)
    candidate = merge_candidate(base, package, selected)
    assets = collect_local_assets(
        Path(args.repo_root).resolve(),
        package,
        Path(args.asset_root or package_path.parent).resolve(),
        selected,
    )
    validate_asset_selection(assets, selected)
    names = {asset["asset"] for asset in assets}
    validate_preserved_assets(base, names, selected)
    rewrite_urls(candidate, names, STAGING_TAG, repository)
    ensure_staging_release(repository)
    index = load_staging_index(repository)
    previous = copy.deepcopy(index)
    for asset in assets:
        upload_missing(repository, STAGING_TAG, asset)

    index["contracts"][relative] = {
        "manifest": candidate,
        "assets": [
            {key: value for key, value in asset.items() if key != "path"}
            for asset in assets
        ],
        "selectors": sorted(selected),
        "sourceCommit": git_commit(Path(args.repo_root).resolve()),
    }
    contract_path = cache_root(args) / "staging" / "contracts" / relative
    with tempfile.TemporaryDirectory(prefix="sgt-staging-publish-") as directory:
        index_path = Path(directory, INDEX_ASSET)
        write_json(index_path, index)
        gh(
            "release",
            "upload",
            STAGING_TAG,
            str(index_path),
            "--repo",
            repository,
            "--clobber",
        )
    verify_index(repository, index)
    write_json(contract_path, candidate)
    remove_unreferenced_staging_assets(repository, previous, index)
    print(contract_path)
    return 0


def git_commit(repo: Path) -> str:
    result = run(["git", "-C", str(repo), "rev-parse", "HEAD"])
    return result.stdout.strip()


def verify_index(repository: str, expected: dict[str, Any]) -> None:
    actual = load_staging_index(repository)
    if actual != expected:
        raise RuntimeError("staging index read-back differs from the uploaded bytes")


def referenced_assets(index: dict[str, Any]) -> set[str]:
    names: set[str] = set()
    for contract in index.get("contracts", {}).values():
        if isinstance(contract, dict):
            for asset in contract.get("assets", []):
                if isinstance(asset, dict) and isinstance(asset.get("asset"), str):
                    names.add(asset["asset"])
    return names


def remove_unreferenced_staging_assets(repository: str, previous: dict[str, Any], current: dict[str, Any]) -> None:
    stale = referenced_assets(previous) - referenced_assets(current)
    assets = release_assets(repository, STAGING_TAG)
    for name in sorted(stale):
        asset = assets.get(name)
        if asset and isinstance(asset.get("id"), int):
            gh("api", "--method", "DELETE", f"repos/{repository}/releases/assets/{asset['id']}")


def promote(args: argparse.Namespace) -> int:
    repository = args.repository
    index = load_staging_index(repository)
    relative = safe_contract_relative(args.contract_relative)
    contract = index["contracts"].get(relative)
    if not isinstance(contract, dict):
        raise ValueError(f"staging has no candidate for {relative}")
    assets = contract.get("assets")
    if not isinstance(assets, list) or not assets:
        raise ValueError(f"staging candidate has no assets for {relative}")
    raw_selectors = contract.get("selectors", [])
    valid_selectors = isinstance(raw_selectors, list) and all(isinstance(selector, str) for selector in raw_selectors)
    if not valid_selectors:
        raise ValueError("staging candidate has invalid selectors")
    selected = parse_selectors(raw_selectors)
    validate_asset_selection(assets, selected)
    staged_manifest = contract.get("manifest")
    if not isinstance(staged_manifest, dict):
        raise ValueError("staging candidate has no valid manifest")
    if args.apply_tracked:
        tracked = Path(args.apply_tracked).resolve()
        base = read_json(tracked)
    else:
        base = staged_manifest
    manifest = merge_candidate(base, staged_manifest, selected)
    names = {asset["asset"] for asset in assets}
    validate_preserved_assets(base, names, selected)
    rewrite_urls(manifest, names, PRODUCTION_TAG, repository)
    with tempfile.TemporaryDirectory(prefix="sgt-promote-") as directory:
        for asset in assets:
            name = asset["asset"]
            path = Path(directory, name)
            download_asset(repository, STAGING_TAG, name, path)
            if path.stat().st_size != asset["sizeBytes"] or sha256(path) != asset["sha256"]:
                raise RuntimeError(f"staging candidate failed verification: {name}")
            publish = dict(asset)
            publish["path"] = str(path)
            upload_missing(repository, PRODUCTION_TAG, publish)

    output = Path(args.output).resolve()
    write_json(output, manifest)
    if args.apply_tracked:
        write_json(tracked, manifest)
    if args.clean_staging:
        previous = copy.deepcopy(index)
        del index["contracts"][relative]
        with tempfile.TemporaryDirectory(prefix="sgt-staging-clean-") as directory:
            index_path = Path(directory, INDEX_ASSET)
            write_json(index_path, index)
            gh(
                "release",
                "upload",
                STAGING_TAG,
                str(index_path),
                "--repo",
                repository,
                "--clobber",
            )
        verify_index(repository, index)
        remove_unreferenced_staging_assets(repository, previous, index)
        remove_local_staged_contract(args, relative)
    print(output)
    return 0


def verify(args: argparse.Namespace) -> int:
    index = load_staging_index(args.repository)
    staging_assets = set(release_assets(args.repository, STAGING_TAG))
    unindexed = staging_assets - referenced_assets(index) - {INDEX_ASSET}
    if unindexed:
        raise RuntimeError(f"staging release has unindexed assets: {', '.join(sorted(unindexed))}")
    for contract in index["contracts"].values():
        for asset in contract.get("assets", []):
            verify_remote(
                args.repository,
                STAGING_TAG,
                asset["asset"],
                asset["sizeBytes"],
                asset["sha256"],
            )
    print(f"Verified {len(referenced_assets(index))} staging assets")
    return 0


def discard(args: argparse.Namespace) -> int:
    repository = args.repository
    index = load_staging_index(repository)
    relative = safe_contract_relative(args.contract_relative)
    if relative not in index["contracts"]:
        if remove_local_staged_contract(args, relative):
            print(f"Discarded stale local staging contract for {relative}")
            return 0
        raise ValueError(f"staging has no candidate for {relative}")
    previous = copy.deepcopy(index)
    del index["contracts"][relative]
    with tempfile.TemporaryDirectory(prefix="sgt-staging-discard-") as directory:
        index_path = Path(directory, INDEX_ASSET)
        write_json(index_path, index)
        gh(
            "release",
            "upload",
            STAGING_TAG,
            str(index_path),
            "--repo",
            repository,
            "--clobber",
        )
    verify_index(repository, index)
    remove_unreferenced_staging_assets(repository, previous, index)
    remove_local_staged_contract(args, relative)
    print(f"Discarded staging candidate for {relative}")
    return 0


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    result.add_argument("--repository", default=REPOSITORY)
    result.add_argument("--cache-root")
    subcommands = result.add_subparsers(dest="action", required=True)

    initialize = subcommands.add_parser("init")
    initialize.set_defaults(handler=lambda args: (ensure_staging_release(args.repository), 0)[1])

    staging = subcommands.add_parser("stage")
    staging.add_argument("--repo-root", default=Path(__file__).resolve().parents[1])
    staging.add_argument("--package-manifest", required=True)
    staging.add_argument("--tracked-manifest", required=True)
    staging.add_argument("--contract-relative", required=True)
    staging.add_argument("--asset-root")
    staging.add_argument("--select", action="append")
    staging.set_defaults(handler=stage)

    promotion = subcommands.add_parser("promote")
    promotion.add_argument("--contract-relative", required=True)
    promotion.add_argument("--output", required=True)
    promotion.add_argument("--apply-tracked")
    promotion.add_argument("--clean-staging", action="store_true")
    promotion.set_defaults(handler=promote)

    verification = subcommands.add_parser("verify-staging")
    verification.set_defaults(handler=verify)

    discard_parser = subcommands.add_parser("discard-staging")
    discard_parser.add_argument("--contract-relative", required=True)
    discard_parser.set_defaults(handler=discard)
    return result


def main() -> int:
    args = parser().parse_args()
    return args.handler(args)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
