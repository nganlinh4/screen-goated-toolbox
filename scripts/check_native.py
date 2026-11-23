"""Run development checks for repository-owned native Cargo packages on Windows."""

import argparse
import os
import re
from pathlib import Path
import subprocess

from scripts.dev_cache_paths import managed_cache_root


def discover_crates(repo):
    result = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard",
         "--", "native/**/Cargo.toml", "native/**/Cargo.lock"],
        cwd=repo, check=True, capture_output=True,
    )
    owned = set(result.stdout.decode("utf-8").split("\0")) - {""}
    crates = {}
    for name in sorted(name for name in owned if name.endswith("/Cargo.toml")):
        path = Path(name)
        if not (repo / path.parent).exists():
            continue  # The whole crate directory was removed: a retired crate, not a broken one.
        if len(path.parts) != 3 or path.parts[0] != "native" or path.name != "Cargo.toml":
            raise ValueError(f"Native check discovery requires an explicit layout decision: {name}")
        manifest = repo / path
        if not manifest.is_file() or manifest.is_symlink() or not manifest.resolve().is_relative_to(repo.resolve()):
            raise ValueError(f"Native manifest is missing or outside the checkout: {name}")
        lock = path.with_name("Cargo.lock")
        if lock.as_posix() not in owned or not (repo / lock).is_file():
            raise ValueError(f"Native locked checks require a non-ignored lockfile: {lock}")
        crates[path.parts[1]] = path.as_posix()
    if not crates:
        raise ValueError("No repository-owned native packages were discovered")
    return crates


def select_crates(crates, requested):
    names = requested or sorted(crates)
    if not names:
        raise ValueError("No native packages were selected")
    for name in names:
        if name not in crates:
            raise ValueError(f"Unknown native package: {name}")
    return sorted(set(names))


def needs_egui_checkouts(repo, crates, selected):
    """Whether a selected package builds from the disposable libs/egui-* checkouts.

    Only then must they be recreated first; packages that do not use them
    build on a clean checkout without that step.
    """
    return any(
        "libs/egui-" in re.sub(r"[\\/]+", "/", (repo / crates[name]).read_text(encoding="utf-8"))
        for name in selected
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--crate", action="append", help="Package directory name; repeat to select several")
    parser.add_argument("--list", action="store_true", help="List selections without building or testing")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    crates = discover_crates(repo)
    selected = select_crates(crates, args.crate)
    for name in selected:
        print(crates[name], flush=True)
    if args.list:
        return
    if os.name != "nt":
        raise ValueError("Native development checks require the Windows toolchain")
    target = managed_cache_root() / "cargo" / "dev"
    if target.resolve().is_relative_to(repo):
        raise ValueError("Native Cargo development artifacts must stay outside the repository")
    environment = {**os.environ, "CARGO_TARGET_DIR": str(target)}

    def run(*command):
        print("Running: " + " ".join(command), flush=True)
        subprocess.run(command, cwd=repo, env=environment, check=True)

    if needs_egui_checkouts(repo, crates, selected):
        run("pwsh", "-NoProfile", "-File", "scripts/ensure-egui-dependencies.ps1")
    for name in selected:
        manifest = crates[name]
        if name == "recorder_worker":
            run("pwsh", "-NoProfile", "-File", "scripts/check-recorder-worker-format.ps1")
        else:
            run("cargo", "fmt", "--manifest-path", manifest, "--check")
        run("cargo", "test", "--manifest-path", manifest, "--locked")
        run("cargo", "clippy", "--manifest-path", manifest, "--all-targets", "--locked", "--", "-D", "warnings")
    print(f"Native development checks passed for {len(selected)} packages", flush=True)


if __name__ == "__main__":
    main()
