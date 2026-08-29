#!/usr/bin/env python3
"""Preview deterministic Help Assistant retrieval from the reviewed corpus."""

from __future__ import annotations

import argparse
import importlib.util
import math
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
BUILDER_PATH = Path(__file__).with_name("help_index_build.py")
TOP_K = 20


def load_builder():
    spec = importlib.util.spec_from_file_location("help_index_build", BUILDER_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load the help index builder")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def score(entry: dict[str, object], terms: list[str]) -> float:
    path = str(entry["path"]).lower()
    text = str(entry["text"]).lower()
    result = 0.0
    for term in terms:
        count = (path + "\n" + text).count(term)
        if count:
            result += 1.0 + math.log(count)
        if term in path:
            result += 3.0
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("question")
    parser.add_argument("--platform", choices=("windows", "android"), default="windows")
    args = parser.parse_args()
    builder = load_builder()
    catalog = builder.DEFAULT_CATALOG.resolve()
    _, documents = builder.read_catalog(catalog)
    entries = [
        entry
        for entry in builder.build_entries(catalog, documents)
        if args.platform in entry["platforms"]
    ]
    terms = [
        term
        for term in "".join(
            character.lower() if character.isalnum() or character == "_" else " "
            for character in args.question
        ).split()
        if len(term) >= 2
    ]
    ranked = entries[:TOP_K] if not terms else sorted(
        ((score(entry, terms), entry) for entry in entries),
        key=lambda item: item[0],
        reverse=True,
    )
    if terms:
        entries = [entry for value, entry in ranked[:TOP_K] if value > 0]
    else:
        entries = ranked
    for index, entry in enumerate(entries, 1):
        print(f"{index}. {entry['title']} ({entry['path']})")
        print(str(entry["text"])[:500])
        print()


if __name__ == "__main__":
    main()
