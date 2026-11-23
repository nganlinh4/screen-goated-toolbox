#!/usr/bin/env python3
"""Report unused Cargo and npm dependencies for the root, native and frontend packages.

Cargo crates are checked with cargo-machete. A crate that compiles shared root
sources through `#[path]` modules (the recorder worker does) would look unused
to cargo-machete, so every crate is copied into a scratch mirror that also holds
the external module files it includes. npm packages are checked with depcheck.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
ROOT_SOURCE_ENTRIES = ("src", "build.rs", "build_support", "tests", "benches", "examples")
SKIPPED_DIRECTORIES = {"target", "node_modules", "dist", ".git"}
PATH_ATTRIBUTE = re.compile(r'#\[path\s*=\s*"([^"]+)"\s*\]')
DEPCHECK = "depcheck@1.4.7"
# Dependencies consumed only through configuration that depcheck cannot parse.
NPM_IGNORES = {
    "screen-record": ["@tailwindcss/postcss", "postcss"],
    "tts-playground-ui": ["autoprefixer", "postcss", "tailwindcss"],
}


def cargo_crates(repo: Path) -> list[Path]:
    native = sorted(path.parent for path in (repo / "native").glob("*/Cargo.toml"))
    return [repo, *native]


def crate_sources(crate: Path, repo: Path) -> list[Path]:
    """Return every Rust source the crate owns, without nested crates."""
    roots = (
        [crate / entry for entry in ROOT_SOURCE_ENTRIES]
        if crate == repo
        else [crate]
    )
    sources: list[Path] = []
    for root in roots:
        if root.is_file() and root.suffix == ".rs":
            sources.append(root)
        elif root.is_dir():
            sources.extend(_rust_files(root, skip_nested_crates=root != crate))
    return sources


def _rust_files(directory: Path, skip_nested_crates: bool = True) -> list[Path]:
    found: list[Path] = []
    for child in sorted(directory.iterdir()):
        if child.is_dir():
            if child.name in SKIPPED_DIRECTORIES:
                continue
            if skip_nested_crates and (child / "Cargo.toml").is_file():
                continue
            found.extend(_rust_files(child))
        elif child.suffix == ".rs":
            found.append(child)
    return found


def module_tree(module_file: Path) -> list[Path]:
    """Over-approximate the files a `#[path]` module can pull in as children."""
    files = [module_file]
    children = (
        module_file.parent
        if module_file.name == "mod.rs"
        else module_file.with_suffix("")
    )
    if children.is_dir():
        files.extend(_rust_files(children))
    return files


def external_modules(sources: list[Path], owned: set[Path]) -> list[Path]:
    """Follow `#[path]` modules that leave the crate, transitively."""
    pending = list(sources)
    seen = {path.resolve() for path in sources}
    external: list[Path] = []
    while pending:
        source = pending.pop()
        text = source.read_text(encoding="utf-8", errors="replace")
        for relative in PATH_ATTRIBUTE.findall(text):
            target = (source.parent / relative).resolve()
            if not target.is_file():
                continue
            for module_file in module_tree(target):
                resolved = module_file.resolve()
                if resolved in seen:
                    continue
                seen.add(resolved)
                pending.append(resolved)
                if resolved not in owned:
                    external.append(resolved)
    return external


def build_mirror(crate: Path, repo: Path, destination: Path) -> Path:
    sources = crate_sources(crate, repo)
    owned = {path.resolve() for path in sources}
    mirror = destination / (crate.name if crate != repo else "_root")
    mirror.mkdir(parents=True)
    shutil.copy2(crate / "Cargo.toml", mirror / "Cargo.toml")
    for source in sources:
        _copy_into(source, crate, mirror)
    for source in external_modules(sources, owned):
        _copy_into(source, repo, mirror / "src" / "sgt_shared_modules")
    return mirror


def _copy_into(source: Path, base: Path, destination: Path) -> None:
    target = destination / source.resolve().relative_to(base.resolve())
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)


def check_cargo(repo: Path, machete: str) -> int:
    executable = shutil.which(machete)
    if executable is None:
        raise SystemExit(
            f"{machete} not found: run `cargo install cargo-machete --version 0.9.2 "
            "--locked` or pass --cargo-machete <path>"
        )
    with tempfile.TemporaryDirectory(prefix="sgt-unused-deps-") as scratch:
        mirrors = [
            build_mirror(crate, repo, Path(scratch))
            for crate in cargo_crates(repo)
        ]
        result = subprocess.run([executable, *map(str, mirrors)], check=False)
    if result.returncode not in (0, 1):
        raise RuntimeError("cargo-machete failed to analyze the crates")
    return result.returncode


def npm_packages(repo: Path) -> list[Path]:
    return sorted(
        path.parent
        for path in repo.glob("*/package-lock.json")
        if (path.parent / "package.json").is_file()
    )


def check_npm(repo: Path, npx: str) -> int:
    failures = 0
    for package in npm_packages(repo):
        command = [npx, "--yes", DEPCHECK, "--json"]
        ignored = NPM_IGNORES.get(package.name, [])
        if ignored:
            command.append("--ignores=" + ",".join(ignored))
        result = subprocess.run(
            command, cwd=package, capture_output=True, text=True, check=False
        )
        if not result.stdout.strip():
            raise RuntimeError(f"depcheck failed for {package.name}: {result.stderr}")
        report = json.loads(result.stdout)
        unused = report["dependencies"] + report["devDependencies"]
        if unused:
            failures += 1
            print(f"{package.name}: unused npm dependencies: {', '.join(unused)}")
        else:
            print(f"{package.name}: no unused npm dependencies")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo-machete", default="cargo-machete")
    parser.add_argument("--npx", default="npx.cmd" if sys.platform == "win32" else "npx")
    parser.add_argument("--skip-cargo", action="store_true")
    parser.add_argument("--skip-npm", action="store_true")
    args = parser.parse_args()
    status = 0
    if not args.skip_cargo:
        status |= check_cargo(REPO, args.cargo_machete)
    if not args.skip_npm:
        status |= check_npm(REPO, args.npx)
    return status


if __name__ == "__main__":
    raise SystemExit(main())
