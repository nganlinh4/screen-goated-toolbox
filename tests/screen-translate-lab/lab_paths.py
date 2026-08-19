"""Locate the screen translate lab artifacts that live outside the repository.

Tooling is tracked here; case inputs, generated viewer data, and run results stay
in the managed development area so real-run artifacts never enter the repository.
Override the artifact root with SGT_SCREEN_TRANSLATE_LAB_ROOT.
"""

from __future__ import annotations

import os
from pathlib import Path

TOOLS = Path(__file__).resolve().parent


def artifact_root() -> Path:
    configured = os.environ.get("SGT_SCREEN_TRANSLATE_LAB_ROOT")
    if configured:
        return Path(configured).resolve()
    local_app_data = os.environ.get("LOCALAPPDATA")
    if not local_app_data:
        raise ValueError("LOCALAPPDATA is required to resolve the lab artifact root")
    return (
        Path(local_app_data) / "SGT-Development" / "manual-tests" / "screen-translate"
    ).resolve()


def inputs_root() -> Path:
    return artifact_root() / "inputs"
