"""Scan the host and repository-owned native lockfiles against RustSec."""

import argparse
import json
from pathlib import Path
import subprocess

from scripts.check_native import discover_crates
from scripts.dev_cache_paths import managed_cache_root


def scan(executable, repo, lockfile, database, fetch):
    command = [str(executable), "audit", "--file", str(lockfile), "--db", str(database), "--json", "--no-yanked"]
    if not fetch:
        command.append("--no-fetch")
    result = subprocess.run(command, cwd=repo, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if result.returncode not in (0, 1):
        raise RuntimeError(f"RustSec scanner failed for {lockfile}: {result.stderr}")
    try:
        report = json.loads(result.stdout)
        count = report["vulnerabilities"]["count"]
        entries = report["vulnerabilities"]["list"]
        if type(count) is not int or not isinstance(entries, list) or count != len(entries):
            raise ValueError("Incomplete vulnerability inventory")
        if result.returncode != int(count > 0):
            raise ValueError("Scanner status disagrees with its vulnerability inventory")
    except (ValueError, TypeError, KeyError) as error:
        raise RuntimeError(f"Invalid RustSec report for {lockfile}: {result.stderr}") from error
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo-audit", type=Path, default=Path("cargo-audit"))
    parser.add_argument("--database", type=Path)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    locks = ["Cargo.lock", *(str(Path(path).with_name("Cargo.lock")) for path in discover_crates(repo).values())]
    if not (repo / "Cargo.lock").is_file():
        raise ValueError("Host Cargo.lock is missing")
    if args.list:
        print("\n".join(locks))
        return
    database = args.database or managed_cache_root() / "tools" / "rustsec-advisory-db"
    reports = {}
    for index, lock in enumerate(locks):
        print(f"Scanning {lock}", flush=True)
        report = scan(args.cargo_audit, repo, lock, database, fetch=index == 0)
        reports[lock] = report
        print(f"  {report['vulnerabilities']['count']} vulnerabilities", flush=True)
    if args.report:
        args.report.write_text(json.dumps(reports, indent=2) + "\n", encoding="utf-8")
    if any(report["vulnerabilities"]["count"] for report in reports.values()):
        raise SystemExit(1)
    print(f"RustSec vulnerability checks passed for {len(locks)} lockfiles", flush=True)


if __name__ == "__main__":
    main()
