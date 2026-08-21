#!/usr/bin/env python3
"""Quality gate for the availability feed.

The feed became the only thing standing between a published endpoint and a user's
retry chain, so it has to judge whether a model is *right*, not merely whether it
answers.

That distinction is the whole point. A dead endpoint is cheap: the retry chain
advances in about half a second, opens a cooldown sized to what the provider
reports, and skips it for free until it reopens. A *confidently wrong* endpoint is
expensive, because nothing downstream catches it: HTTP 200, no error, no retry,
wrong answer on the user's screen.

Two such failures were observed and are gated here:

- Both NVIDIA vision endpoints transcribe Vietnamese with every diacritic
  stripped, turning `PHỐ` into `PHO`. A Latin-only fixture scores that as a pass.
- `muse-glimmer-30b` answers correctly after seventeen seconds, which is accurate
  and useless as a fallback.

A third was suspected and did not survive checking. `riva-translate-4b-instruct-v2`
answers in English when driven through its documented language-pair system prompt
(`{"role": "system", "content": "ja-vi"}`), but that is not the shape this product
sends. Given an ordinary instruction it translates Japanese to Vietnamese
correctly. The non-English-source case below stays regardless, because that is the
direction the product actually uses and an English-source suite never exercises it.
"""

from __future__ import annotations

import re
import unicodedata

# Bumped whenever the checks below change. Scores only aggregate samples taken
# under the same gate: mixing them reports a rate for a test that was never run
# as a whole, which is how a tightened gate briefly published every model at a
# quarter success.
GATE_VERSION = 3

# Latency above which a model is not worth offering even when it is correct.
# muse-glimmer answers accurately at ~17s; a fallback that slow is not a fallback.
MAX_ACCEPTABLE_P50_MS = 6_000

# Vietnamese tone and vowel marks. Their absence in a Vietnamese reply is the
# signature of a model that transliterates instead of transcribing.
_VIETNAMESE_MARKS = re.compile(
    "[ăâđêôơưĂÂĐÊÔƠƯ]|[̣̀́̃̉]"
)
_HANGUL = re.compile(r"[가-힣]")
_CJK = re.compile(r"[一-鿿]")


def has_vietnamese_marks(text: str) -> bool:
    """Whether the text carries Vietnamese diacritics."""
    decomposed = unicodedata.normalize("NFD", text)
    return bool(_VIETNAMESE_MARKS.search(decomposed))


def looks_like_english(text: str) -> bool:
    """Whether a reply is plain ASCII prose, i.e. never translated at all."""
    letters = [c for c in text if c.isalpha()]
    if not letters:
        return False
    ascii_share = sum(1 for c in letters if c.isascii()) / len(letters)
    return ascii_share > 0.95


# Each case states what a correct reply must look like, so a wrong-language reply
# fails rather than scoring partial credit for containing the right nouns.
CASES = (
    {
        "id": "en-vi-ui",
        "prompt": (
            "Translate to Vietnamese, output only the translation: "
            '"Settings > Display > Night light. Turn on automatically at sunset."'
        ),
        "expect_any": ("cài đặt", "màn hình", "hiển thị", "đèn", "ánh sáng"),
        "require_marks": "vi",
    },
    {
        "id": "en-vi-sentence",
        "prompt": (
            "Translate to Vietnamese, output only the translation: "
            '"The battery is low. Connect the charger to continue."'
        ),
        "expect_any": ("pin", "sạc", "yếu", "kết nối"),
        "require_marks": "vi",
    },
    {
        # The failure this exists for: an any-to-English model asked for a
        # non-English target. riva-translate answers such a request in fluent
        # English, which every English-source case scores as a pass. The product
        # translates arbitrary source into the user's language, so this is the
        # shape that matters, not en->vi.
        "id": "ja-vi",
        "prompt": (
            "Translate to Vietnamese, output only the translation: "
            "電源ボタンを長押ししてください。"
        ),
        "expect_any": (),
        "require_marks": "vi",
    },
    {
        "id": "en-ko",
        "prompt": (
            "Translate to Korean, output only the translation: "
            '"The battery is low. Connect the charger to continue."'
        ),
        "expect_any": (),
        "require_marks": "ko",
    },
)


def judge(case: dict, reply: str) -> tuple[bool, str]:
    """Whether one reply satisfies its case, and why not when it fails."""
    if not reply or not reply.strip():
        return False, "empty"

    lowered = reply.lower()
    marks = case["require_marks"]

    if marks == "vi":
        # The wrong-language failure: fluent, plausible, and not the language asked for.
        if looks_like_english(reply):
            return False, "answered in English"
        if not has_vietnamese_marks(reply):
            return False, "no Vietnamese diacritics"
    elif marks == "ko":
        if not _HANGUL.search(reply):
            return False, "no Hangul"
        if _CJK.search(reply):
            return False, "answered in Chinese characters"

    if case["expect_any"] and not any(token in lowered for token in case["expect_any"]):
        return False, "no expected term"
    return True, ""


def verdict(results: list[tuple[bool, str]], p50_ms: int | None) -> tuple[bool, str]:
    """Whether a model passes the gate, and the reason when it does not.

    Every case must pass. A model that translates one sentence correctly and
    another into the wrong language is not usable, and averaging would hide that.
    """
    if not results:
        return False, "no successful call"
    for ok, reason in results:
        if not ok:
            return False, reason
    if p50_ms is None:
        return False, "no latency"
    if p50_ms > MAX_ACCEPTABLE_P50_MS:
        return False, f"too slow ({p50_ms}ms)"
    return True, ""


# --- vision ---------------------------------------------------------------
#
# Vision endpoints are gated on the failure that actually disqualified them:
# transcribing Vietnamese with the diacritics stripped. Both NVIDIA vision models
# read Latin text perfectly and returned `PHO` for `PHỐ`, which a Latin-only
# fixture would have scored as a pass.

VISION_CASES = (
    {
        "id": "ocr-ascii",
        "image": "tests/catalog-benchmark/images/ocr/12-near-duplicate-filenames.png",
        "mime": "image/png",
        "instruction": (
            "Transcribe both file names exactly as they appear, left to right, "
            "one per line. Output ONLY the text."
        ),
        "expect_all": ("213759.png", "213630.png"),
        "require_marks": None,
    },
    {
        # The exact diacritical forms, not merely "some diacritic somewhere".
        # Stripping is partial in practice: one model returned `PHO` beside a
        # correct `NHÀ CHUNG`, another returned `HANOI` beside a correct `PHỐ`.
        # Both satisfy a presence test while losing the characters that carry
        # meaning, so the words themselves are asserted.
        "id": "ocr-vietnamese",
        "image": "tests/catalog-benchmark/images/ocr/01-vietnamese-street-sign.jpg",
        "mime": "image/jpeg",
        "instruction": "Extract all text from this image exactly as it appears. Output ONLY the text.",
        "expect_all": ("hà nội", "phố", "nhà chung"),
        "require_marks": "vi",
    },
)


def judge_vision(case: dict, reply: str) -> tuple[bool, str]:
    """Whether one OCR reply satisfies its case."""
    if not reply or not reply.strip():
        return False, "empty"
    lowered = reply.lower()
    for token in case["expect_all"]:
        if token.lower() not in lowered:
            return False, f"missing {token}"
    if case["require_marks"] == "vi" and not has_vietnamese_marks(reply):
        return False, "transcribed without Vietnamese diacritics"
    return True, ""


if __name__ == "__main__":  # pragma: no cover - exercised by monitor_quality_tests
    raise SystemExit("this module is imported by monitor_nvidia_models.py")
