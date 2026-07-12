"""Fail when a repository Markdown link points at a missing local path."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote


ROOT = Path(__file__).resolve().parent.parent
LINK = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
SCHEME = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*:")


def markdown_files() -> list[Path]:
    result = subprocess.run(
        [
            "git",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            "*.md",
        ],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return [ROOT / line for line in result.stdout.splitlines() if (ROOT / line).is_file()]


def local_target(raw: str) -> str | None:
    value = raw.strip()
    if value.startswith("<") and ">" in value:
        value = value[1 : value.index(">")]
    else:
        # Markdown permits an optional quoted title after a whitespace separator.
        value = re.split(r'\s+["\']', value, maxsplit=1)[0]
    value = unquote(value).split("#", 1)[0].split("?", 1)[0]
    if not value or value.startswith(("#", "//")) or SCHEME.match(value):
        return None
    return value


def main() -> int:
    failures: list[str] = []
    for doc in markdown_files():
        for line_number, line in enumerate(doc.read_text(encoding="utf-8").splitlines(), 1):
            for match in LINK.finditer(line):
                target = local_target(match.group(1))
                if target is None:
                    continue
                candidate = Path(target)
                if not candidate.is_absolute():
                    candidate = doc.parent / candidate
                if not candidate.exists():
                    failures.append(
                        f"{doc.relative_to(ROOT)}:{line_number}: missing {target}"
                    )

    claude = ROOT / ".claude" / "CLAUDE.md"
    expected = "Read and follow `../AGENTS.md`."
    if claude.read_text(encoding="utf-8").strip() != expected:
        failures.append(".claude/CLAUDE.md: must contain only the AGENTS.md redirect")

    if failures:
        print("Documentation check failed:")
        print("\n".join(f"- {failure}" for failure in failures))
        return 1

    print(f"Documentation check passed ({len(markdown_files())} Markdown files).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
