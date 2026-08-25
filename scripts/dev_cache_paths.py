"""Path guards shared by deterministic package builders."""

from __future__ import annotations

import os
from pathlib import Path


def managed_cache_root() -> Path:
    configured = os.environ.get("SGT_DEV_CACHE_ROOT")
    if configured:
        return Path(configured).resolve()
    local_app_data = os.environ.get("LOCALAPPDATA")
    if not local_app_data:
        raise ValueError("LOCALAPPDATA is required to resolve the managed dev cache")
    return (Path(local_app_data) / "SGT-Development" / "cache").resolve()


def require_repo_or_managed_cache(repo: Path, path: Path, label: str) -> Path:
    resolved = path.resolve()
    try:
        resolved.relative_to(repo.resolve())
        return resolved
    except ValueError:
        pass
    cache = managed_cache_root()
    try:
        relative = resolved.relative_to(cache)
    except ValueError as error:
        raise ValueError(
            f"{label} must stay inside the repository or managed dev cache"
        ) from error
    if not relative.parts or relative.parts[0] not in {
        "cargo",
        "packages",
        "evidence",
        "performance",
        "staging",
    }:
        raise ValueError(f"{label} is outside an allowed managed-cache area")
    return resolved


def manifest_path(repo: Path, path: Path) -> str:
    try:
        return path.resolve().relative_to(repo.resolve()).as_posix()
    except ValueError:
        return str(path.resolve())
