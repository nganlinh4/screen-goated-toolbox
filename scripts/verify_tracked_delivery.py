#!/usr/bin/env python3
"""Require a read-back-generated delivery contract to match the tracked authority."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def load(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise SystemExit(f"missing delivery contract: {path}") from error
    except json.JSONDecodeError as error:
        raise SystemExit(f"invalid delivery contract {path}: {error}") from error


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("generated", type=Path)
    parser.add_argument("tracked", type=Path)
    args = parser.parse_args()

    if load(args.generated) != load(args.tracked):
        raise SystemExit(
            "read-back delivery differs from tracked authority: "
            f"{args.generated} != {args.tracked}"
        )
    print(f"Verified tracked delivery: {args.tracked}")


if __name__ == "__main__":
    main()
