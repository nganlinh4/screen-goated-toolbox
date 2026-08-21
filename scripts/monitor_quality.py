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

A dedicated translation endpoint can correctly pass this translation suite, but
that evidence qualifies it only as a translator. The generic availability
publisher omits dedicated capabilities; passing this suite never makes a
translation-only endpoint a generic text model. The non-English-source case
remains because it exercises a product direction that an English-source suite
never reaches.
"""

from __future__ import annotations

import re
import unicodedata

# Bumped whenever the checks below change. Scores only aggregate samples taken
# under the same gate: mixing them reports a rate for a test that was never run
# as a whole, which is how a tightened gate briefly published every model at a
# quarter success.
GATE_VERSION = 5

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


def invented_placeholder(source: str, reply: str) -> str | None:
    """A bracketed placeholder the reply invented, if any.

    Structural on purpose. A model that decides it is drafting an email finishes
    the template it imagined -- `[Your Name]`, `[Chuc vu]`, `[Contact]` -- and no
    list of phrases can keep up with the ways it phrases that. What every case
    shares is the shape: bracketed text in the reply that the source never
    contained cannot be a translation of anything.
    """
    for match in re.finditer(r"[\[［]([^\]］]{1,40})[\]］]", reply):
        if match.group(0) not in source and match.group(1).strip():
            return match.group(0)
    return None


def echoes_the_source(source: str, reply: str, run: int = 20) -> bool:
    """Whether the reply hands back a long verbatim run of the source.

    A model that returns its input untranslated passes any check that only asks
    what the reply contains. Short runs are legitimate -- a proper noun or a
    technical term carries through -- so only a run no translation would preserve
    counts.
    """
    compact_source = "".join(source.split())
    compact_reply = "".join(reply.split())
    if len(compact_source) < run:
        return False
    return any(
        compact_source[index : index + run] in compact_reply
        for index in range(len(compact_source) - run + 1)
    )


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
        # A translation preset receives copied documents, not isolated sentences.
        # Opaque identifiers and distinct blocks must survive while prose changes
        # language; otherwise a fluent-looking fragment can hide lost content.
        "id": "ko-vi-structured",
        "prompt": (
            "Translate the following text to Vietnamese. Output ONLY the translation.\n\n"
            "@atlas_notes\n"
            "3 years ago (edited)\n"
            "와 말도 안 된다, 정말 멋있다. 나만 울컥한 건가? "
            "모두 고생했고 누구 하나 빠지지 않고 등장했다. "
            "주인공이 스포트라이트를 받아야 하지만 멋지게 양보했고, "
            "이 영상에 모든 노력이 담긴 것 같아 다시 울컥한다. "
            "그런데 여기에는 한국 사람이 없나?\n\n"
            "1.4K\n\n"
            "Reply\n\n"
            "3 replies\n\n"
            "@river_team\n"
            "3 years ago\n"
            "영상이 여러 팀을 중심으로 구성된 것도 새롭고 모든 팀이 "
            "함께 등장한 것도 정말 잘한 것 같다. 솔직히 전부 나올 줄 "
            "몰랐는데 놀랍다.\n\n"
            "1.8K\n\n"
            "Reply"
        ),
        "expect_any": (),
        "require_marks": "vi",
        "forbid_source_script": "ko",
        "preserve_all": ("@atlas_notes", "@river_team", "1.4K", "1.8K"),
        "minimum_nonempty_lines": 8,
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
        # A translation endpoint can silently choose the wrong target language,
        # which every English-source case may score as a pass. The product
        # translates arbitrary source into the user's language, so this is the
        # shape that matters, not only en->vi.
        "id": "ja-vi",
        "prompt": (
            "Translate to Vietnamese, output only the translation: "
            "電源ボタンを長押ししてください。"
        ),
        "expect_any": (),
        "require_marks": "vi",
    },
    {
        # The shape that exposed instruction drift in production: a real business
        # note, long enough that a model can decide it is drafting a document
        # rather than translating one. Short probes never provoke that.
        "id": "ko-vi-note",
        "prompt": (
            "Translate the following text to Vietnamese. Output ONLY the translation.\n\n"
            "대표님, 요청하신 검체별 결과 요약에 "
            "수검자명과 점수 컴럼을 추가했습니다. "
            "첨부 화면은 제 개인 테스트 환경에서 "
            "캡처한 것으로, 표시된 수치는 실제 "
            "값과 다릅니다. 확인 부탁드립니다."
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

    for literal in case.get("preserve_all", ()):
        if literal not in reply:
            return False, f"lost opaque content {literal}"

    if case.get("minimum_nonempty_lines") is not None:
        lines = sum(1 for line in reply.splitlines() if line.strip())
        if lines < case["minimum_nonempty_lines"]:
            return False, f"collapsed document structure ({lines} lines)"

    residue = reply
    for literal in case.get("preserve_all", ()):
        residue = residue.replace(literal, "")
    if case.get("forbid_source_script") == "ko" and _HANGUL.search(residue):
        return False, "left Korean prose untranslated"

    # Structural checks, applied to every case rather than stated per case: they
    # describe how a reply relates to its request, so they hold for inputs
    # nobody wrote a case for.
    source = case["prompt"]
    invented = invented_placeholder(source, reply)
    if invented is not None:
        return False, f"invented placeholder {invented}"
    if echoes_the_source(source, reply):
        return False, "returned the source untranslated"
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
