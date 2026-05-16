#!/usr/bin/env python
"""Frozen launcher for the managed Step Audio EditX runtime.

The launcher is built with PyInstaller and delegates execution to the bundled
Python environment so dependency changes do not require rebuilding the launcher
itself.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


def main() -> int:
    root = Path(sys.executable).resolve().parent
    python = root / "python_runtime" / "Scripts" / "python.exe"
    sidecar = root / "sidecar" / "step_audio_sidecar.py"
    if not python.is_file():
        print(f"[step-audio-launcher] Missing Python runtime: {python}", file=sys.stderr)
        return 2
    if not sidecar.is_file():
        print(f"[step-audio-launcher] Missing sidecar script: {sidecar}", file=sys.stderr)
        return 3
    env = os.environ.copy()
    env.setdefault("PYTHONUTF8", "1")
    env.setdefault("PYTHONNOUSERSITE", "1")
    process = subprocess.run(
        [str(python), str(sidecar)],
        stdin=sys.stdin,
        stdout=sys.stdout,
        stderr=sys.stderr,
        env=env,
        check=False,
    )
    return int(process.returncode)


if __name__ == "__main__":
    raise SystemExit(main())
