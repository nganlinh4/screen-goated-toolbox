"""Build the help-assistant RAG index from the codebase.

Chunks source files, embeds each via a KaLM-compatible embedding server,
saves to help-index.json.

Set KALM_EMBED_SERVER_URL to override the default endpoint.
Run: python scripts/help_index_build.py
"""

import json, os, sys, time, math, requests, pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
INDEX_PATH = ROOT / "help-index.json"
EMBED_SERVER_URL = os.environ.get("KALM_EMBED_SERVER_URL", "http://127.0.0.1:8400/api/embed")
MAX_CHARS_PER_CHUNK = 32000  # ~8k tokens per chunk
BATCH_SIZE = 50  # chunks per batch (Ollama handles sequentially, no rate limit)

# Full codebase — everything users might ask about
INCLUDE_DIRS = [
    "src", "screen-record/src", "bilingual-relay-ui/src", "catalog",
    "libs/lang-detect/src", "native/qwen3_runtime/src",
    "mobile/androidApp/src/main/java", "mobile/shared/src/commonMain",
    "promptdj-midi",
]
INCLUDE_FILES = [
    "Cargo.toml", "README.md", "build.rs",
    "screen-record/package.json", "screen-record/tsconfig.json",
    "mobile/androidApp/build.gradle.kts", "mobile/shared/build.gradle.kts",
    "mobile/androidApp/src/main/AndroidManifest.xml",
]
EXCLUDE_PATTERNS = {
    "node_modules", "target", "dist", ".git", ".claude", "parity-fixtures",
    "third_party", "scripts", "src/embed_dlls",
    "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/carousel",
    "mobile/androidApp/src/test", "mobile/androidApp/src/androidTest",
    "mobile/shared/src/commonTest", "mobile/shared/src/androidUnitTest",
    "mobile/androidApp/src/main/res", "mobile/androidApp/src/main/assets",
}
INCLUDE_EXTS = {".rs", ".ts", ".tsx", ".js", ".jsx", ".json", ".toml", ".md",
                 ".css", ".html", ".kt", ".kts", ".xml", ".py"}



def should_include(path: pathlib.Path) -> bool:
    rel = path.relative_to(ROOT).as_posix()
    parts = rel.split("/")
    # Exclude if any path segment matches an exclude pattern
    for ex in EXCLUDE_PATTERNS:
        if ex in parts or rel.startswith(ex + "/") or rel == ex:
            return False
    if path.suffix not in INCLUDE_EXTS:
        return False
    return True


def collect_files():
    files = []
    for inc_dir in INCLUDE_DIRS:
        d = ROOT / inc_dir
        if not d.exists():
            continue
        for f in sorted(d.rglob("*")):
            if f.is_file() and should_include(f):
                files.append(f)
    for inc_file in INCLUDE_FILES:
        f = ROOT / inc_file
        if f.is_file():
            files.append(f)
    return files


def chunk_file(path: pathlib.Path):
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except Exception:
        return []
    if not text.strip():
        return []
    rel = path.relative_to(ROOT).as_posix()
    # Split large files into multiple chunks
    chunks = []
    if len(text) <= MAX_CHARS_PER_CHUNK:
        chunks.append({"path": rel, "text": text})
    else:
        lines = text.split("\n")
        buf, buf_len = [], 0
        part = 0
        for line in lines:
            if buf_len + len(line) > MAX_CHARS_PER_CHUNK and buf:
                chunks.append({"path": f"{rel}#part{part}", "text": "\n".join(buf)})
                part += 1
                buf, buf_len = [], 0
            buf.append(line)
            buf_len += len(line) + 1
        if buf:
            chunks.append({"path": f"{rel}#part{part}" if part > 0 else rel, "text": "\n".join(buf)})
    return chunks


def embed_one(text: str) -> list[float]:
    """Embed a single text via the configured KaLM-compatible server."""
    for attempt in range(3):
        try:
            resp = requests.post(
                EMBED_SERVER_URL,
                json={"input": text[:16000]},
                timeout=120,
            )
            if resp.status_code == 200:
                embeddings = resp.json().get("embeddings", [])
                return embeddings[0] if embeddings else []
            if resp.status_code == 500 and attempt < 2:
                # Truncate more aggressively on OOM
                text = text[:len(text)//2]
                print(f"  500 error, retrying with {len(text)} chars...")
                time.sleep(2)
                continue
            print(f"  Embed error {resp.status_code}: {resp.text[:200]}")
            return []
        except requests.exceptions.ConnectionError:
            if attempt < 2:
                print(f"  Connection error, retrying in 5s...")
                time.sleep(5)
                continue
            return []
    return []


def main():
    print(f"Collecting files from {ROOT}...")
    files = collect_files()
    print(f"Found {len(files)} files")

    chunks = []
    for f in files:
        chunks.extend(chunk_file(f))
    print(f"Split into {len(chunks)} chunks")
    print(f"Batch size: {BATCH_SIZE}, estimated batches: {math.ceil(len(chunks)/BATCH_SIZE)}")

    # Group small files in same directory into single chunks
    merged = []
    dir_buf = {}
    for c in chunks:
        d = c["path"].rsplit("/", 1)[0] if "/" in c["path"] else ""
        if len(c["text"]) > 8000:
            # Large file gets its own chunk
            merged.append(c)
        else:
            if d not in dir_buf:
                dir_buf[d] = {"path": d + "/*", "text": ""}
            combined = dir_buf[d]["text"] + f"\n\n// === {c['path']} ===\n" + c["text"]
            if len(combined) > MAX_CHARS_PER_CHUNK:
                merged.append(dir_buf[d])
                dir_buf[d] = {"path": d + "/*", "text": f"// === {c['path']} ===\n" + c["text"]}
            else:
                dir_buf[d]["text"] = combined
    for leftover in dir_buf.values():
        if leftover["text"].strip():
            merged.append(leftover)
    chunks = merged
    print(f"After merging small files: {len(chunks)} chunks")

    # Resume from partial index if exists
    index = []
    done_paths = set()
    if INDEX_PATH.exists():
        existing = json.loads(INDEX_PATH.read_text(encoding="utf-8"))
        index = existing
        done_paths = {e["path"] for e in existing}
        print(f"Resuming: {len(done_paths)} chunks already embedded")
    chunks = [c for c in chunks if c["path"] not in done_paths]
    print(f"Remaining: {len(chunks)} chunks to embed")

    # Embed each chunk via the configured KaLM-compatible server.
    for i, chunk in enumerate(chunks):
        embed_text = f"File: {chunk['path']}\n\n{chunk['text']}"
        vec = embed_one(embed_text)
        if not vec:
            print(f"  SKIP {chunk['path']} (embed failed)")
            continue
        index.append({"path": chunk["path"], "text": chunk["text"], "vector": vec})
        if (i + 1) % 50 == 0 or i == len(chunks) - 1:
            print(f"  [{i+1}/{len(chunks)}] {len(index)} embedded")

    # Save
    INDEX_PATH.write_text(json.dumps(index), encoding="utf-8")
    size_mb = INDEX_PATH.stat().st_size / 1024 / 1024
    print(f"\nDone! {len(index)} chunks → {INDEX_PATH.name} ({size_mb:.1f} MB)")


if __name__ == "__main__":
    main()
