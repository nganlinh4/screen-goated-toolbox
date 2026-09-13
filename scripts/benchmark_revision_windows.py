"""Replay public-speech provider events; no application input or network access.

Compare a frozen-word-prefix policy against unrestricted final reconciliation.
Quality distances use the provider's final text, NOT a human reference or WER.
"""
from __future__ import annotations

import argparse
import difflib
import json
import math
import re
from pathlib import Path


def tokens(text):
    return re.findall(r"\s+|\w+(?:['’]\w+)*|[^\w\s]", text, re.UNICODE)


def word_tokens(text):
    return re.findall(r"\w+(?:['’]\w+)*", text.casefold(), re.UNICODE)


def distance(reference, actual):
    previous = list(range(len(actual) + 1))
    for i, left in enumerate(reference, 1):
        current = [i]
        for j, right in enumerate(actual, 1):
            current.append(min(current[-1] + 1, previous[j] + 1,
                               previous[j - 1] + (left != right)))
        previous = current
    return previous[-1]


def common_prefix(left, right):
    count = 0
    for a, b in zip(left, right):
        if a != b:
            break
        count += 1
    return count


class Window:
    def __init__(self, words):
        self.words = words
        self.visible = ""
        self.locked = 0
        self.rejected = 0

    def update(self, incoming):
        if self.words is None:
            self.visible = incoming
            return incoming
        old, new = tokens(self.visible), tokens(incoming)
        starts = [i for i, token in enumerate(old) if re.match(r"\w", token)]
        boundary = starts[-self.words] if len(starts) > self.words else 0
        self.locked = max(self.locked, boundary)
        frozen = old[:self.locked]
        result = []
        for kind, a, b, c, d in difflib.SequenceMatcher(
                None, old, new, autojunk=False).get_opcodes():
            if kind == "equal":
                result.extend(old[a:b])
            elif a < self.locked:
                # Reject an entire edit hunk touching frozen words. Never clip
                # by character offsets, which can produce partial-word fragments.
                result.extend(old[a:b])
                self.rejected += 1
            else:
                result.extend(new[c:d])
        assert result[:self.locked] == frozen
        self.visible = "".join(result)
        return self.visible

    def finish(self):
        self.visible = ""
        self.locked = 0


def read_events(path):
    events = []
    for line in path.read_text(encoding="utf-8-sig").splitlines():
        if "PROBE_FRAME " in line:
            event = json.loads(line.split("PROBE_FRAME ", 1)[1])
            if event.get("final") or event.get("interim"):
                events.append(event)
    return events


def segment_text(committed, text):
    if not committed:
        return text.lstrip()
    return (" " if text and not committed[-1].isspace()
            and not text[0].isspace() else "") + text


def percentile(values, percent):
    return sorted(values)[max(0, math.ceil(len(values) * percent) - 1)] if values else 0


def evaluate(events, words, *, policy_override=None, final_override=False):
    policy = policy_override if policy_override is not None else Window(words)
    committed = ""
    reference = ""
    edits = []
    losses = []
    frames = []
    segment_start = 0
    for event in events:
        final = bool(event.get("final"))
        raw = event["final"] if final else event["interim"]
        incoming = segment_text(reference, raw)
        # Match the application's streaming control-character sanitization.
        incoming = "".join(" " if ord(c) < 32 or ord(c) == 127
                           or c in "\u2028\u2029" else c for c in incoming)
        old = policy.visible
        if final and final_override:
            policy.visible = incoming
            visible = incoming
        else:
            visible = policy.update(incoming)
        prefix = common_prefix(old, visible)
        backspaces = len(old) - prefix
        inserted = len(visible) - prefix
        frames.append({"ms": event["ms"], "final": final,
                       "backspaces": backspaces, "inserted": inserted,
                       "old_chars": len(old), "new_chars": len(visible)})
        if backspaces:
            edits.append(backspaces)
        if final:
            if incoming != visible:
                losses.append({"ms": event["ms"], "expected": incoming,
                               "actual": visible,
                               "word_distance": distance(word_tokens(incoming), word_tokens(visible))})
            reference += incoming
            committed += visible
            policy.finish()
            segment_start = event["ms"]
    reference_words, actual_words = word_tokens(reference), word_tokens(committed)
    word_difference = distance(reference_words, actual_words)
    return {
        "window_words": words, "events": len(events),
        "final_segments": sum(bool(e.get("final")) for e in events),
        "last_final_ms": segment_start, "pending_tail_chars": len(policy.visible),
        "reference_words": len(reference_words), "actual_words": len(actual_words),
        "word_distance_from_provider_final": word_difference,
        "word_distance_percent": round(100 * word_difference / max(1, len(reference_words)), 3),
        "correction_batches": len(edits), "total_backspaces": sum(edits),
        "max_backspaces": max(edits, default=0), "p95_backspaces": percentile(edits, .95),
        "batches_over_80_chars": sum(n > 80 for n in edits),
        "batches_over_200_chars": sum(n > 200 for n in edits),
        "final_backspaces": sum(f["backspaces"] for f in frames if f["final"]),
        "interim_backspaces": sum(f["backspaces"] for f in frames if not f["final"]),
        "rejected_edit_hunks": policy.rejected, "changed_final_segments": len(losses),
        "final_text": committed, "provider_final_text": reference,
        "pending_tail": policy.visible, "losses": losses, "frames": frames,
    }


def self_test():
    assert distance(["a", "b"], ["a", "c", "d"]) == 2
    policy = Window(2)
    policy.update("one two three four")
    assert policy.update("ONE two three five six") == "one two three five six"
    assert policy.update("ONE two THREE five seven") == "one two three five seven"
    policy.finish()
    assert policy.update("new segment") == "new segment"
    policy = Window(1)
    policy.update("repeat repeat old")
    assert policy.update("repeat repeat corrected") == "repeat repeat corrected"
    assert policy.update("insert repeat repeat corrected more").startswith("repeat repeat")
    events = [{"ms": 1, "interim": " a b c", "final": None},
              {"ms": 2, "interim": "ignored", "final": "a b d"}]
    result = evaluate(events, None)
    assert result["final_text"] == "a b d"
    assert result["word_distance_from_provider_final"] == 0
    assert result["pending_tail_chars"] == 0
    # A later hypothesis can carry an already committed prefix. Freezing it
    # prevents the final from removing that transient duplication.
    events = [{"ms": 1, "final": "alpha beta gamma"},
              {"ms": 2, "interim": "alpha beta gamma delta epsilon zeta"},
              {"ms": 3, "final": "delta epsilon zeta"}]
    assert evaluate(events, None)["word_distance_from_provider_final"] == 0
    bounded = evaluate(events, 2)
    assert bounded["word_distance_from_provider_final"] == 3
    assert bounded["final_text"] == "alpha beta gamma alpha beta gamma delta epsilon zeta"
    policy = Window(2)
    policy.update("first second third fourth")
    assert policy.update("first changed fourth") == "first second third fourth"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logs", nargs="*", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("revision-window self-tests passed")
    if not args.logs:
        return
    assert args.output is not None, "--output is required"
    args.output.mkdir(parents=True, exist_ok=True)
    summaries = []
    for path in args.logs:
        events = read_events(path)
        assert events, f"no transcript frames in {path}"
        for words in [None, 5, 10, 20, 30, 40, 60, 80]:
            result = evaluate(events, words)
            result["source_log"] = str(path.resolve())
            name = f"{path.stem}-{words or 'unbounded'}"
            (args.output / f"{name}.json").write_text(
                json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
            (args.output / f"{name}.txt").write_text(result["final_text"], encoding="utf-8")
            summaries.append({k: v for k, v in result.items()
                              if k not in {"final_text", "provider_final_text", "pending_tail", "losses", "frames"}})
    (args.output / "summary.json").write_text(json.dumps(summaries, indent=2), encoding="utf-8")
    print(json.dumps(summaries, indent=2))


if __name__ == "__main__":
    main()
