"""Fail when a parity spec cites a repository file that does not exist.

Parity specs under .claude/parity name their canonical sources as inline code
(`src/...`, `mobile/...`, `parity-fixtures/...`). Markdown link checking does not
see those, so a moved file leaves the spec pointing at a dead path.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SPEC_DIR = Path(".claude/parity")
INLINE_CODE = re.compile(r"`([^`\s]+)`")
FILE_SUFFIX = re.compile(r"\.[A-Za-z0-9]{1,5}$")


def cited_path(token: str, root: Path) -> str | None:
    """Return the repository-relative file a code span cites, or None."""
    value = token.split("#", 1)[0]
    value = re.sub(r":\d+(?:-\d+)?$", "", value).rstrip(".,;)")
    if "/" not in value or "://" in value or value.startswith(("/", "~", ".")):
        return None
    if any(marker in value for marker in ("*", "{", "<", "...")):
        return None
    if not FILE_SUFFIX.search(value):
        return None
    # Only spans rooted at an existing top-level entry are repository paths;
    # anything else (MIME types, package names, relative examples) is prose.
    if not (root / value.split("/", 1)[0]).exists():
        return None
    return value


def missing_spec_paths(root: Path = ROOT) -> list[str]:
    missing: list[str] = []
    for spec in sorted((root / SPEC_DIR).glob("*.md")):
        lines = spec.read_text(encoding="utf-8").splitlines()
        for number, line in enumerate(lines, 1):
            for token in INLINE_CODE.findall(line):
                path = cited_path(token, root)
                if path is not None and not (root / path).exists():
                    missing.append(f"{spec.name}:{number}: {path}")
    return missing


def main() -> int:
    missing = missing_spec_paths()
    for entry in missing:
        print(f"missing parity spec path: {entry}", file=sys.stderr)
    return 1 if missing else 0


if __name__ == "__main__":
    raise SystemExit(main())
