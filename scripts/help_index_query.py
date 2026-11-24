#!/usr/bin/env python3
"""Check semantic retrieval against a locally built Help Assistant source index."""

from __future__ import annotations

import argparse
import base64
import gzip
import heapq
import json
import math
from array import array
from pathlib import Path

from catalog_model_discovery import credential_pool, read_dotenv
from help_index_build import ROOT
from help_index_embeddings import DIMENSIONS, Gemini


def rank(path: Path, platform: str, question: str, keys: list[str], count: int):
    with gzip.open(path, "rt", encoding="utf-8") as index:
        metadata = json.loads(next(index))
        if metadata["schemaVersion"] != 2 or metadata["dimensions"] != DIMENSIONS:
            raise ValueError("Unsupported help source index")
        client = Gemini(metadata["embeddingModel"], keys)
        query = array("b", client.embed(["task: question answering | query: " + question])[0])
        query_norm = math.hypot(*query)

        def matches():
            for line in index:
                entry = json.loads(line)
                if platform in entry["platforms"]:
                    vector = array("b", base64.b64decode(entry["vector"], validate=True))
                    dot = sum(left * right for left, right in zip(query, vector, strict=True))
                    yield dot / (query_norm * math.hypot(*vector)), entry

        return heapq.nlargest(count, matches(), key=lambda match: match[0])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("question")
    parser.add_argument("--index", type=Path, required=True)
    parser.add_argument("--platform", choices=("windows", "android"), default="windows")
    parser.add_argument("--dotenv", type=Path, default=ROOT / ".env")
    parser.add_argument("--top", type=int, default=6)
    args = parser.parse_args()
    keys = credential_pool("GEMINI_API_KEY", read_dotenv(args.dotenv))
    if not keys:
        parser.error("Set GEMINI_API_KEY or configure the local Gemini key pool")
    for score, entry in rank(args.index, args.platform, args.question, keys, args.top):
        print(f"{score:.3f}  {entry['path']}:{entry['startLine']}")


if __name__ == "__main__":
    main()
