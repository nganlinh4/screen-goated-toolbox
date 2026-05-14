#!/usr/bin/env python
"""Frozen launcher for the managed Magpie runtime.

The launcher is built with PyInstaller and deliberately does not import NeMo.
It delegates stdin/stdout/stderr to the bundled Python environment so package
updates do not require rebuilding the launcher itself.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


def main() -> int:
    root = Path(sys.executable).resolve().parent
    python = root / "python_runtime" / "Scripts" / "python.exe"
    script = root / "sidecar" / "magpie_sidecar.py"
    if not python.is_file():
        print(f"[magpie-launcher] Missing Python runtime: {python}", file=sys.stderr)
        return 2
    if not script.is_file():
        print(f"[magpie-launcher] Missing sidecar script: {script}", file=sys.stderr)
        return 3
    env = os.environ.copy()
    env.setdefault("PYTHONUTF8", "1")
    env.setdefault("PYTHONNOUSERSITE", "1")
    process = subprocess.run(
        [str(python), str(script)],
        stdin=sys.stdin,
        stdout=sys.stdout,
        stderr=sys.stderr,
        env=env,
        check=False,
    )
    return int(process.returncode)


if __name__ == "__main__":
    raise SystemExit(main())
