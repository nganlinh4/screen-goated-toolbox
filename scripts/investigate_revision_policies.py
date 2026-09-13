"""Offline counterfactuals for revision policy; overlap matching is heuristic.

No production behavior is changed. No future final is used by online policies.
"""
import argparse
import difflib
import json
import re
from pathlib import Path

from benchmark_revision_windows import Window, evaluate, read_events, tokens, word_tokens


class CosmeticWindow(Window):
    """Suppress distant case/punctuation-only hunks, allow all lexical edits."""
    def update(self, incoming):
        old, new = tokens(self.visible), tokens(incoming)
        starts = [i for i, token in enumerate(old) if re.match(r"\w", token)]
        boundary = starts[-self.words] if len(starts) > self.words else 0
        result = []
        for kind, a, b, c, d in difflib.SequenceMatcher(
                None, old, new, autojunk=False).get_opcodes():
            cosmetic = word_tokens("".join(old[a:b])) == word_tokens("".join(new[c:d]))
            if kind != "equal" and a < boundary and cosmetic:
                result.extend(old[a:b])
                self.rejected += 1
            else:
                result.extend(new[c:d])
        self.visible = "".join(result)
        return self.visible


def trim_overlap(previous, incoming, minimum=4):
    """Strip a matching committed suffix from an interim, not from a final.

    This cannot distinguish real repeated speech: callers must treat it as a
    diagnostic hypothesis, not an authoritative audio-alignment operation.
    """
    old = word_tokens(previous)
    spans = list(re.finditer(r"\w+(?:['’]\w+)*", incoming))
    new = [m.group().casefold() for m in spans]
    for count in range(min(len(old), len(new)), minimum - 1, -1):
        if old[-count:] == new[:count]:
            end = spans[count].start() if count < len(spans) else len(incoming)
            return incoming[end:], count
    return incoming, 0


def transform(events, overlap=False, hold_words=0):
    previous = ""
    last_interim = ""
    finalized_snapshot = ""
    output = []
    overlaps = []
    for event in events:
        result = dict(event)
        if event.get("final"):
            previous += " " + event["final"]
            finalized_snapshot = last_interim
            last_interim = ""
        elif event.get("interim"):
            text = event["interim"]
            last_interim = text
            if overlap:
                text, count = trim_overlap(previous, text)
                if overlap == "snapshot":
                    snapshot_text, snapshot_count = trim_overlap(finalized_snapshot, event["interim"])
                    if snapshot_count > count:
                        text, count = snapshot_text, snapshot_count
                if count:
                    overlaps.append({"ms": event["ms"], "words": count,
                                     "before": event["interim"], "after": text})
            if hold_words:
                spans = list(re.finditer(r"\w+(?:['’]\w+)*", text))
                text = text[:spans[-hold_words].start()] if len(spans) > hold_words else ""
            result["interim"] = text
            if not text:
                # Represent an empty hypothesis explicitly for the evaluator.
                result["interim"] = " "
        output.append(result)
    return output, overlaps


def raw_events(path):
    result = []
    for line in path.read_text(encoding="utf-8").splitlines():
        row = json.loads(line)
        content = row.get("content", {})
        interim = content.get("interimInputTranscription", {}).get("text")
        final = content.get("inputTranscription", {}).get("text")
        if interim or final:
            result.append({"ms": row["ms"], "interim": interim, "final": final})
    return result


def self_test():
    assert trim_overlap("one two three four", "one two three four five")[0] == "five"
    assert trim_overlap("one two three four", "other words")[0] == "other words"
    # A genuinely repeated clause demonstrates why text-only dedup is unsafe.
    repeated = [{"ms": 1, "final": "one two three four"},
                {"ms": 2, "interim": "one two three four five six seven eight nine ten"},
                {"ms": 3, "final": "one two three four five six seven eight nine ten"}]
    transformed, _ = transform(repeated, overlap=True)
    assert evaluate(repeated, 2)["word_distance_from_provider_final"] == 0
    assert evaluate(transformed, 2)["word_distance_from_provider_final"] == 4
    cosmetic = CosmeticWindow(2)
    cosmetic.update("one two three four")
    assert cosmetic.update("ONE two three five") == "one two three five"
    assert cosmetic.update("not two three five") == "not two three five"
    # Word budgets alone do not bound the erase length of unspaced text.
    long_word = Window(5)
    long_word.update("a" * 400)
    assert long_word.update("b" * 400) == "b" * 400
    # A late meaning-changing correction can fall outside the hard window.
    strict = Window(2)
    strict.update("we will send the package tomorrow morning")
    assert strict.update("we will not send the package tomorrow morning") == (
        "we will send the package tomorrow morning")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logs", nargs="*", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("policy investigation self-tests passed")
    if not args.logs:
        return
    assert args.output
    args.output.mkdir(parents=True, exist_ok=True)
    summary = []
    for path in args.logs:
        events = raw_events(path) if path.suffix == ".jsonl" else read_events(path)
        assert events
        for overlap in [False, True, "snapshot"]:
            for hold in [0, 2, 4]:
                stream, overlaps = transform(events, overlap, hold)
                for window in [None, 5, 8, 10, 12, 15, 20]:
                    for variant in (["hard", "final_override", "cosmetic"] if window else ["hard"]):
                        policy = CosmeticWindow(window) if variant == "cosmetic" else None
                        result = evaluate(stream, window, policy_override=policy,
                                          final_override=variant == "final_override")
                        result.update(sample=path.stem, overlap_heuristic=overlap, variant=variant,
                                      hold_words=hold, overlap_frames=len(overlaps))
                        name = f"{path.stem}-overlap{overlap}-hold{hold}-window{window}-{variant}"
                        (args.output / f"{name}.json").write_text(
                            json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
                        summary.append({k: v for k, v in result.items() if k not in
                                        {"final_text", "provider_final_text", "pending_tail", "losses", "frames"}})
                if overlap and not hold:
                    (args.output / f"{path.stem}-overlaps-{overlap}.json").write_text(
                        json.dumps(overlaps, ensure_ascii=False, indent=2), encoding="utf-8")
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(f"Wrote {len(summary)} policy comparisons")


if __name__ == "__main__":
    main()
