"""Batch Gemini text embeddings and persist completed work by content digest."""

import base64
import hashlib
import itertools
import json
import math
import sqlite3
import threading
import time
import urllib.error
import urllib.request
from concurrent.futures import FIRST_COMPLETED, ThreadPoolExecutor, wait


DIMENSIONS = 768
BATCH_SIZE = 16


class Gemini:
    def __init__(self, model, keys):
        self.model = f"models/{model}"
        self.url = f"https://generativelanguage.googleapis.com/v1beta/{self.model}:batchEmbedContents"
        self.keys = itertools.cycle(keys)
        self.lock = threading.Lock()

    def embed(self, texts):
        requests = [
            {
                "model": self.model,
                "content": {"parts": [{"text": text}]},
                "outputDimensionality": DIMENSIONS,
            }
            for text in texts
        ]
        body = json.dumps({"requests": requests}, ensure_ascii=False).encode()
        for attempt in range(7):
            with self.lock:
                key = next(self.keys)
            request = urllib.request.Request(
                self.url, body, {"Content-Type": "application/json", "x-goog-api-key": key}
            )
            try:
                with urllib.request.urlopen(request, timeout=60) as response:
                    embeddings = json.load(response)["embeddings"]
                if len(embeddings) != len(requests):
                    raise ValueError("Gemini returned the wrong embedding count")
                return [pack_vector(embedding["values"]) for embedding in embeddings]
            except urllib.error.HTTPError as error:
                status = error.code
                retry_after = error.headers.get("Retry-After", "")
                error.close()
                if status not in {401, 403, 408, 429, 500, 502, 503, 504} or attempt == 6:
                    raise RuntimeError(f"Gemini embedding request failed (HTTP {status})") from None
                if status in {401, 403}:
                    continue
                delay = int(retry_after) if retry_after.isdecimal() else 0
                time.sleep(max(delay, min(2 ** attempt, 16)))
            except (TimeoutError, urllib.error.URLError):
                if attempt == 6:
                    raise RuntimeError("Gemini embedding request timed out") from None
                time.sleep(min(2 ** attempt, 16))


def pack_vector(values):
    if len(values) != DIMENSIONS or not all(map(math.isfinite, values)):
        raise ValueError("Gemini returned an invalid embedding")
    peak = max(map(abs, values))
    if not peak:
        raise ValueError("Gemini returned a zero embedding")
    return bytes(round(value / peak * 127) & 255 for value in values)


def attach_embeddings(entries, model, keys, cache_path, workers):
    prompts = [
        f"title: {entry['path']} | text: {entry['text']}"
        for entry in entries
    ]
    identity = f"{model}:{DIMENSIONS}:packed-cosine-v1\0"
    digests = [
        hashlib.sha256((identity + prompt).encode()).digest() for prompt in prompts
    ]
    gemini = Gemini(model, keys)
    started = time.monotonic()
    with sqlite3.connect(cache_path) as cache:
        cache.execute("CREATE TABLE IF NOT EXISTS vectors (digest BLOB PRIMARY KEY, value BLOB NOT NULL)")
        required = dict(zip(digests, prompts))
        vectors = {
            digest: value
            for digest, value in cache.execute("SELECT digest, value FROM vectors")
            if digest in required and len(value) == DIMENSIONS and any(value)
        }
        missing = {digest: prompt for digest, prompt in required.items() if digest not in vectors}
        total = len(missing)
        print(f"Gemini embeddings: {total} to request, {len(vectors)} cached, {len(keys)} configured keys", flush=True)
        remaining_items = iter(missing.items())
        batches = iter(lambda: list(itertools.islice(remaining_items, BATCH_SIZE)), [])
        with ThreadPoolExecutor(max_workers=workers) as pool:
            def submit(batch):
                return pool.submit(gemini.embed, [prompt for _, prompt in batch])

            pending = {submit(batch): batch for batch in itertools.islice(batches, workers)}
            completed = 0
            failure = None
            while pending:
                finished, _ = wait(pending, return_when=FIRST_COMPLETED)
                for future in finished:
                    batch = pending.pop(future)
                    try:
                        embedded = future.result()
                    except (RuntimeError, ValueError) as error:
                        failure = failure or error
                        continue
                    updates = [(digest, vector) for (digest, _), vector in zip(batch, embedded, strict=True)]
                    cache.executemany("INSERT OR REPLACE INTO vectors VALUES (?, ?)", updates)
                    cache.commit()
                    vectors.update(updates)
                    previous = completed
                    completed += len(updates)
                    if completed == total or completed // 200 != previous // 200:
                        elapsed = time.monotonic() - started
                        remaining = round((total - completed) * elapsed / completed)
                        print(f"Embedded {completed}/{total} in {elapsed:.0f}s, estimated {remaining}s remaining", flush=True)
                    if failure is None and (next_batch := next(batches, None)):
                        pending[submit(next_batch)] = next_batch
            if failure is not None:
                raise failure
    for entry, digest in zip(entries, digests, strict=True):
        entry["vector"] = base64.b64encode(vectors[digest]).decode("ascii")
