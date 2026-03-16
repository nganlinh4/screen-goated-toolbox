#!/usr/bin/env python3
"""Flag likely Vietnamese UI strings that were written without diacritics."""

from __future__ import annotations

import argparse
import os
import pathlib
import re
import sys
from dataclasses import dataclass


DEFAULT_EXTENSIONS = {
    ".kt",
    ".java",
    ".js",
    ".ts",
    ".tsx",
    ".jsx",
    ".json",
    ".html",
    ".css",
    ".xml",
    ".md",
}

SKIP_DIRS = {
    ".git",
    ".gradle",
    ".kotlin",
    "build",
    "dist",
    "node_modules",
    "target",
}

VI_DIACRITIC_CHARS = set("àáạảãâầấậẩẫăằắặẳẵèéẹẻẽêềếệểễìíịỉĩòóọỏõôồốộổỗơờớợởỡùúụủũưừứựửữỳýỵỷỹđÀÁẠẢÃÂẦẤẬẨẪĂẰẮẶẲẴÈÉẸẺẼÊỀẾỆỂỄÌÍỊỈĨÒÓỌỎÕÔỒỐỘỔỖƠỜỚỢỞỠÙÚỤỦŨƯỪỨỰỬỮỲÝỴỶỸĐ")

SUSPICIOUS_PHRASES = {
    "preset yeu thich",
    "giu mo",
    "nho hon",
    "lon hon",
    "chua co",
    "hay danh dau sao",
    "dang doi ket qua",
    "dang tai",
    "dang stream",
    "san sang",
    "da sao chep",
    "nhap tai day",
    "enter de gui",
    "xuong dong",
    "se giu mo",
    "overlay nhap chua san sang",
    "ket qua html chua ho tro",
    "tu dan chua ho tro",
    "phim tat chua ho tro",
    "chi ho tro graph",
    "chon text",
    "nhap text",
    "am thanh thiet bi",
}

SUSPICIOUS_TOKENS = {
    "anh",
    "am",
    "ban",
    "chep",
    "chon",
    "chua",
    "copy",
    "dang",
    "danh",
    "day",
    "de",
    "dong",
    "doi",
    "giu",
    "graph",
    "gui",
    "hay",
    "ho",
    "hon",
    "html",
    "ket",
    "loi",
    "lon",
    "mo",
    "ngoai",
    "ngoa",
    "nhap",
    "nho",
    "phim",
    "placeholder",
    "preset",
    "qua",
    "realtime",
    "san",
    "sang",
    "sao",
    "search",
    "stream",
    "tai",
    "tat",
    "text",
    "thanh",
    "thich",
    "thiet",
    "tro",
    "truoc",
    "tu",
    "van",
    "yeu",
    "xuong",
}

STRING_RE = re.compile(
    r'"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'',
)


@dataclass
class Finding:
    path: pathlib.Path
    line_no: int
    score: int
    reasons: list[str]
    snippet: str


def iter_files(root: pathlib.Path, targets: list[str]) -> list[pathlib.Path]:
    if targets:
        files: list[pathlib.Path] = []
        for target in targets:
            path = (root / target).resolve() if not pathlib.Path(target).is_absolute() else pathlib.Path(target)
            if path.is_file():
                files.append(path)
            elif path.is_dir():
                for file_path in path.rglob("*"):
                    if should_scan(file_path):
                        files.append(file_path)
        return sorted(set(files))

    files: list[pathlib.Path] = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [name for name in dirnames if name not in SKIP_DIRS]
        base = pathlib.Path(dirpath)
        for filename in filenames:
            path = base / filename
            if should_scan(path):
                files.append(path)
    return files


def should_scan(path: pathlib.Path) -> bool:
    if not path.is_file():
        return False
    if path.suffix.lower() not in DEFAULT_EXTENSIONS:
        return False
    return not any(part in SKIP_DIRS for part in path.parts)


def extract_strings(line: str) -> list[str]:
    literals = []
    for match in STRING_RE.finditer(line):
        literal = match.group(0)[1:-1]
        if literal.strip():
            literals.append(literal)
    return literals


def score_text(text: str) -> tuple[int, list[str]]:
    if any(ch in VI_DIACRITIC_CHARS for ch in text):
        return 0, []

    normalized = " ".join(text.lower().split())
    if len(normalized) < 4 or not re.search(r"[a-zA-Z]", normalized):
        return 0, []

    phrase_hits = [phrase for phrase in SUSPICIOUS_PHRASES if phrase in normalized]
    tokens = re.findall(r"[a-z]+", normalized)
    token_hits = sorted({token for token in tokens if token in SUSPICIOUS_TOKENS})

    score = len(phrase_hits) * 4
    if len(token_hits) >= 3:
        score += len(token_hits)

    reasons: list[str] = []
    if phrase_hits:
        reasons.append("phrases=" + ",".join(phrase_hits[:3]))
    if len(token_hits) >= 3:
        reasons.append("tokens=" + ",".join(token_hits[:6]))

    if "localized(" in normalized:
        score += 2
        reasons.append("localized-call")

    if len(phrase_hits) >= 1 or score >= 6:
        return score, reasons
    return 0, []


def scan_file(path: pathlib.Path, root: pathlib.Path) -> list[Finding]:
    findings: list[Finding] = []
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        return findings

    rel_path = path.relative_to(root) if path.is_relative_to(root) else path
    for line_no, line in enumerate(text.splitlines(), start=1):
        for literal in extract_strings(line):
            score, reasons = score_text(literal)
            if score:
                findings.append(
                    Finding(
                        path=rel_path,
                        line_no=line_no,
                        score=score,
                        reasons=reasons,
                        snippet=literal.strip(),
                    ),
                )
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="*", help="Optional files or directories to scan")
    args = parser.parse_args()

    root = pathlib.Path.cwd()
    findings: list[Finding] = []
    for path in iter_files(root, args.paths):
        findings.extend(scan_file(path, root))

    findings.sort(key=lambda item: (-item.score, str(item.path), item.line_no))

    if not findings:
        print("No likely unaccented Vietnamese strings found.")
        return 0

    for finding in findings:
        reason_text = "; ".join(finding.reasons)
        print(f"{finding.path}:{finding.line_no}: score={finding.score} {reason_text}")
        print(f"  {finding.snippet}")

    print(f"\nFound {len(findings)} likely unaccented Vietnamese string(s).")
    return 1


if __name__ == "__main__":
    sys.exit(main())
