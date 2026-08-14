from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Any


def read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"JSON root must be an object: {path}")
    return value


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(value, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    os.replace(temporary, path)


def cache_root(args: argparse.Namespace) -> Path:
    if args.cache_root:
        return Path(args.cache_root).resolve()
    configured = os.environ.get("SGT_DEV_CACHE_ROOT")
    if configured:
        return Path(configured).resolve()
    local = os.environ.get("LOCALAPPDATA")
    if not local:
        raise ValueError("LOCALAPPDATA is unavailable; pass --cache-root")
    return Path(local, "SGT-Development", "cache").resolve()


def safe_contract_relative(value: str) -> str:
    normalized = value.replace("\\", "/")
    parts = normalized.split("/")
    if (
        not normalized
        or normalized.startswith("/")
        or any(not part or part in {".", ".."} or ":" in part for part in parts)
        or not normalized.endswith(".json")
    ):
        raise ValueError("--contract-relative must be a safe JSON path")
    return normalized


def remove_local_staged_contract(args: argparse.Namespace, relative: str) -> bool:
    root = cache_root(args) / "staging" / "contracts"
    target = root.joinpath(*relative.split("/"))
    if target.is_symlink() or (target.exists() and not target.is_file()):
        raise ValueError(f"local staged contract is unsafe: {target}")
    if not target.exists():
        return False
    target.unlink()
    parent = target.parent
    while parent != root:
        try:
            parent.rmdir()
        except OSError:
            break
        parent = parent.parent
    return True
