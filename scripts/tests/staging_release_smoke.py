"""Opt-in live smoke for the mutable staging release."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "component_release.py"
CONTRACT = "component-delivery/tests/staging-smoke-v1.json"


def invoke(*arguments: str) -> None:
    subprocess.run(
        ["py", "-3", str(SCRIPT), *arguments],
        cwd=REPO,
        check=True,
    )


def main() -> int:
    if os.environ.get("SGT_RUN_STAGING_SMOKE") != "1":
        raise SystemExit("set SGT_RUN_STAGING_SMOKE=1 for the live GitHub smoke")
    with tempfile.TemporaryDirectory(prefix="sgt-staging-smoke-") as directory:
        root = Path(directory)
        payload = b"SGT mutable staging smoke\n"
        digest = hashlib.sha256(payload).hexdigest()
        name = f"sgt-staging-smoke-1.0.0-{digest[:16]}.bin"
        asset = root / name
        asset.write_bytes(payload)
        tracked = root / "tracked.json"
        package = root / "packages.json"
        tracked.write_text(
            json.dumps(
                {
                    "schemaVersion": 1,
                    "architecture": "x64",
                    "component": {
                        "id": "staging-smoke",
                        "version": "0.0.0",
                        "asset": "unused-0000000000000000.bin",
                        "downloadUrl": "https://example.invalid/unused.bin",
                        "sizeBytes": 1,
                        "sha256": "0" * 64,
                    },
                }
            ),
            encoding="utf-8",
        )
        package.write_text(
            json.dumps(
                {
                    "schemaVersion": 1,
                    "architecture": "x64",
                    "component": {
                        "id": "staging-smoke",
                        "version": "1.0.0",
                        "asset": name,
                        "assetPath": str(asset),
                        "sizeBytes": len(payload),
                        "sha256": digest,
                    },
                }
            ),
            encoding="utf-8",
        )
        try:
            invoke(
                "--cache-root",
                str(root / "cache"),
                "stage",
                "--package-manifest",
                str(package),
                "--tracked-manifest",
                str(tracked),
                "--contract-relative",
                CONTRACT,
                "--asset-root",
                str(root),
            )
            invoke("verify-staging")
        finally:
            invoke("discard-staging", "--contract-relative", CONTRACT)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
