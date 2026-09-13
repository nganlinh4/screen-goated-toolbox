"""Compare replay output with a marker-bounded published text reference.

This is a text-reference distance, not manually audio-aligned ground truth.
"""
import argparse
from html.parser import HTMLParser
import json
from pathlib import Path

from benchmark_revision_windows import distance, word_tokens


class PlainText(HTMLParser):
    def __init__(self):
        super().__init__()
        self.parts = []

    def handle_data(self, data):
        self.parts.append(data)


def locate(words, marker, start=0):
    for i in range(start, len(words) - len(marker) + 1):
        if words[i:i + len(marker)] == marker:
            return i
    raise ValueError("reference marker not found")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("--start", required=True)
    parser.add_argument("--end", required=True)
    parser.add_argument("--results", type=Path, required=True)
    parser.add_argument("--pattern", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    document = PlainText()
    document.feed(args.reference.read_text(encoding="utf-8-sig"))
    words = word_tokens(" ".join(document.parts))
    first, last = word_tokens(args.start), word_tokens(args.end)
    start = locate(words, first)
    end = locate(words, last, start) + len(last)
    reference = words[start:end]
    rows = []
    for path in sorted(args.results.glob(args.pattern)):
        result = json.loads(path.read_text(encoding="utf-8"))
        if "final_text" not in result:
            continue
        differences = distance(reference, word_tokens(result["final_text"]))
        rows.append({"file": path.name, "reference_words": len(reference),
                     "word_edits": differences,
                     "text_reference_distance_percent": round(100 * differences / len(reference), 3),
                     "window": result["window_words"], "variant": result["variant"],
                     "overlap": result["overlap_heuristic"], "hold": result["hold_words"]})
    args.output.write_text(json.dumps(rows, indent=2), encoding="utf-8")
    print(f"Compared {len(rows)} outputs against {len(reference)} reference words")


if __name__ == "__main__":
    main()
