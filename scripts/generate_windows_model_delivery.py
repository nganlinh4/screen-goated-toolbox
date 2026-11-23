#!/usr/bin/env python3
"""Generate the reviewed Windows model-delivery catalog.

Archive inventories come only from the deterministic output of
package_windows_models.py. This script performs no network access.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def generate_delivery(packages: dict) -> dict:
    """Return the tracked delivery for the packaged models, in package order.

    No Windows model ships today; the list is empty until a new offline engine
    adds a spec to package_windows_models.PACKAGES (docs/OFFLINE-ENGINES.md).
    """
    if packages.get("schemaVersion") != 1 or not isinstance(packages.get("models"), list):
        raise ValueError("unsupported package-manifest schema")
    ids = [item["id"] for item in packages["models"]]
    if len(ids) != len(set(ids)):
        raise ValueError(f"duplicate package models: {sorted(ids)}")
    return {"schemaVersion": 1, "models": packages["models"]}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--package-manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    packages = json.loads(args.package_manifest.read_text(encoding="utf-8"))
    output = generate_delivery(packages)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    print(args.output)
    for model in output["models"]:
        print(f"{model['id']}: {model['installedSizeBytes']} bytes, {len(model['files'])} files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
