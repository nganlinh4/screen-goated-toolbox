#!/usr/bin/env python3
"""Install one immutable creation-runtime artifact after exact verification."""

from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import urllib.request


CHUNK_SIZE = 128 * 1024


def identity(path: Path) -> tuple[int, str] | None:
    if not path.is_file():
        return None
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as source:
        while chunk := source.read(CHUNK_SIZE):
            size += len(chunk)
            digest.update(chunk)
    return size, digest.hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--byte-count", required=True, type=int)
    parser.add_argument("--sha256", required=True)
    args = parser.parse_args()

    expected = (args.byte_count, args.sha256)
    if args.byte_count <= 0 or len(args.sha256) != 64:
        raise SystemExit("Creation runtime identity is invalid")
    if identity(args.output) == expected:
        return

    args.output.parent.mkdir(parents=True, exist_ok=True)
    partial = args.output.with_name(f"{args.output.name}.part")
    partial.unlink(missing_ok=True)
    digest = hashlib.sha256()
    written = 0
    try:
        request = urllib.request.Request(args.url, headers={"User-Agent": "SGT-Android-Build"})
        with urllib.request.urlopen(request, timeout=120) as source, partial.open("wb") as target:
            while chunk := source.read(CHUNK_SIZE):
                written += len(chunk)
                if written > args.byte_count:
                    raise RuntimeError("Creation runtime download exceeds its byte contract")
                digest.update(chunk)
                target.write(chunk)
            target.flush()
            os.fsync(target.fileno())
        if (written, digest.hexdigest()) != expected:
            raise RuntimeError("Downloaded creation runtime failed identity validation")
        os.replace(partial, args.output)
    finally:
        partial.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
