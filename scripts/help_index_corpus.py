"""Select, scrub, and split repository source for Help Assistant indexing."""

from __future__ import annotations

import re
import subprocess
from collections import Counter
from pathlib import Path, PurePosixPath


TEXT_EXTENSIONS = set("""
 .aidl .bat .c .cjs .cmake .cmd .cpp .css .frag .gradle .h .hpp .html .java
 .js .json .jsx .kt .kts .md .mjs .patch .pro .properties .ps1 .py .rc .rs
 .sh .sql .toml .ts .tsx .txt .vert .wgsl .xml .yaml .yml
""".split())
EXCLUDED_DIRS = {
    ".agents", ".git", "build", "dist", "generated", "node_modules",
    "target", "third_party", "vendor", "viewer-dist",
}
EXCLUDED_NAMES = {
    "agents.md", "package-lock.json", "pnpm-lock.yaml", "yarn.lock",
    "i18n_scan_report.json", "local.properties", "gradle-wrapper.properties",
}
MAX_CHUNK_CHARS = 4500
OVERLAP_LINES = 5
SECRET = re.compile(
    r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY-----"
    r"|\bAIza[\w-]{35}\b|\bgh[pousr]_[A-Za-z0-9]{36,}\b"
    r"|\bgithub_pat_[A-Za-z0-9_]{50,}\b|\b(?:AKIA|ASIA)[A-Z0-9]{16}\b"
    r"|\b(?:sk-or-v1-|sk-proj-|gsk_|nvapi-)[A-Za-z0-9_-]{24,}\b"
    r'|"(?:private_key|client_secret)"\s*:\s*"(?:[^"\\]|\\.){16,}"'
)
URL_CREDENTIAL = re.compile(r"(https?://)[^\s/@:]+:[^\s/@]+@")


def excluded(path: PurePosixPath) -> bool:
    name = path.name.lower()
    parts = tuple(part.lower() for part in path.parts)
    return (
        any(part in EXCLUDED_DIRS for part in parts[:-1])
        or (parts[0] == ".claude" and parts[:2] != (".claude", "parity"))
        or parts[0] in {"assets", "component-notices"}
        or parts[:2] == ("tests", "catalog-benchmark")
        or parts[:6] == ("mobile", "androidapp", "src", "main", "assets", "promptdj")
        or (parts[0] == "component-delivery" and name.startswith("help-index-"))
        or name in EXCLUDED_NAMES
        or name == "golden.json"
        or name.startswith((".env", "tmp-release-notes"))
        or ".local." in name
        or name.endswith((".min.js", ".min.css", ".bundle.js"))
        or "service-account" in name
        or "keystore" in name
        or (path.suffix.lower() not in TEXT_EXTENSIONS and name != "makefile")
    )


def platforms_for(path: PurePosixPath) -> list[str]:
    if path.as_posix() == "docs/help/windows-guide.md":
        return ["windows"]
    if path.as_posix() == "docs/help/android-guide.md":
        return ["android"]
    if path.parts[0] == "src":
        return ["windows"]
    if path.parts[:2] in {("mobile", "androidApp"), ("native", "android")}:
        return ["android"]
    return ["android", "windows"]


def chunks(text: str):
    """Keep code near its surrounding lines and also handle long single lines."""
    lines = [
        (number, line[offset : offset + MAX_CHUNK_CHARS])
        for number, line in enumerate(text.splitlines(keepends=True), 1)
        for offset in range(0, len(line), MAX_CHUNK_CHARS)
    ]
    cursor = 0
    while cursor < len(lines):
        end = cursor
        length = 0
        while end < len(lines) and length + len(lines[end][1]) <= MAX_CHUNK_CHARS:
            length += len(lines[end][1])
            end += 1
        content = "".join(line for _, line in lines[cursor:end]).strip()
        if content:
            yield lines[cursor][0], content
        cursor = end if end == len(lines) else max(cursor + 1, end - OVERLAP_LINES)


def collect(repo: Path, ref: str) -> tuple[list[dict], Counter]:
    paths = subprocess.check_output(
        ["git", "ls-tree", "-rz", "--full-tree", ref], cwd=repo
    )
    entries = []
    counts = Counter()
    with subprocess.Popen(
        ["git", "cat-file", "--batch"],
        cwd=repo,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
    ) as git:
        for record in filter(None, paths.split(b"\0")):
            meta, path_bytes = record.split(b"\t", 1)
            mode, kind, oid = meta.split()
            path = PurePosixPath(path_bytes.decode("utf-8"))
            if mode not in {b"100644", b"100755"} or kind != b"blob" or excluded(path):
                counts["excluded"] += 1
                continue
            git.stdin.write(oid + b"\n")
            git.stdin.flush()
            header = git.stdout.readline().split()
            if len(header) != 3 or header[:2] != [oid, b"blob"]:
                raise ValueError(f"Git could not read source blob: {path}")
            size = int(header[2])
            raw = git.stdout.read(size)
            if len(raw) != size or git.stdout.read(1) != b"\n":
                raise ValueError(f"Git returned incomplete source: {path}")
            try:
                text = raw.decode("utf-8-sig").replace("\r\n", "\n")
            except UnicodeDecodeError:
                counts["non_text"] += 1
                continue
            if "\0" in text or text.startswith("version https://git-lfs.github.com/spec/v1\n"):
                counts["non_text"] += 1
                continue
            if SECRET.search(text):
                counts["credential_match"] += 1
                continue
            text, redacted = URL_CREDENTIAL.subn(r"\1[redacted]@", text)
            counts["redacted_url_credentials"] += redacted
            pieces = [
                {"path": path.as_posix(), "startLine": line, "platforms": platforms_for(path), "text": body}
                for line, body in chunks(text)
            ]
            if pieces:
                counts["files"] += 1
                counts["source_chars"] += len(text)
                entries.extend(pieces)
        git.stdin.close()
        if git.wait():
            raise RuntimeError("Git source inventory failed")
    return entries, counts
