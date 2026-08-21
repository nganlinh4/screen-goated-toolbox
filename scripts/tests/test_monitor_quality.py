"""The availability feed must reject confidently wrong answers as well as
transport failures, using the same prompt and judgment path as production
monitoring.
"""

import importlib.util
from pathlib import Path
import sys
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "monitor_quality.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("monitor_quality", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)

NOTE_CASE = next(case for case in MODULE.CASES if case["id"] == "ko-vi-note")
STRUCTURED_CASE = next(case for case in MODULE.CASES if case["id"] == "ko-vi-structured")
SOURCE = NOTE_CASE["prompt"].split(chr(10) + chr(10), 1)[1]


class InventedPlaceholderTest(unittest.TestCase):
    def test_flags_a_template_the_source_never_contained(self) -> None:
        reply = "Toi da bo sung cot diem so." + chr(10) + "Tran trong," + chr(10) + "[Your Name]"
        self.assertEqual(MODULE.invented_placeholder(SOURCE, reply), "[Your Name]")

    def test_ignores_brackets_the_source_actually_has(self) -> None:
        # A bracket carried through from the request is a translation, not an
        # invention, so the rule compares against the source rather than
        # rejecting brackets outright.
        source = "Nhan [OK] de tiep tuc."
        self.assertIsNone(MODULE.invented_placeholder(source, "Press [OK] to continue."))

    def test_ignores_an_empty_bracket(self) -> None:
        self.assertIsNone(MODULE.invented_placeholder(SOURCE, "so lieu [ ] chua ro"))


class EchoTest(unittest.TestCase):
    def test_flags_a_reply_that_hands_the_source_back(self) -> None:
        self.assertTrue(MODULE.echoes_the_source(SOURCE, SOURCE))

    def test_a_short_carried_over_term_is_not_an_echo(self) -> None:
        # Technical terms and proper nouns legitimately survive translation.
        source = "diem so XGB Z-score la chi so rui ro"
        reply = "Diem so cua mo hinh XGB Z-score nam trong khoang 0 den 1."
        self.assertFalse(MODULE.echoes_the_source(source, reply))


class JudgeTest(unittest.TestCase):
    """The gate as the monitor calls it, with representative replies."""

    def test_a_correct_vietnamese_reply_passes(self) -> None:
        reply = (
            "Tôi đã bổ sung thêm cột tên người được kiểm tra và điểm số vào báo cáo "
            "tổng hợp kết quả mẫu theo yêu cầu. Ảnh đính kèm là kết quả thu thập "
            "được trong môi trường thử nghiệm cá nhân của tôi."
        )
        ok, why = MODULE.judge(NOTE_CASE, reply)
        self.assertTrue(ok, why)

    def test_an_invented_signature_block_fails(self) -> None:
        reply = (
            "Kính gửi Quý Trưởng Ban," + chr(10)
            + "Tôi đã bổ sung cột điểm số." + chr(10)
            + "Trân trọng," + chr(10) + "[Your Name]"
        )
        ok, why = MODULE.judge(NOTE_CASE, reply)
        self.assertFalse(ok)
        self.assertIn("placeholder", why)

    def test_answering_in_english_fails(self) -> None:
        ok, why = MODULE.judge(NOTE_CASE, "The score is a risk indicator from 0 to 1.")
        self.assertFalse(ok)
        self.assertEqual(why, "answered in English")

    def test_returning_the_source_untranslated_fails(self) -> None:
        ok, why = MODULE.judge(NOTE_CASE, SOURCE)
        self.assertFalse(ok)

    def test_a_complete_structured_translation_passes(self) -> None:
        reply = (
            "@atlas_notes\n3 năm trước (đã chỉnh sửa)\n"
            "Cảnh này thật tuyệt vời và thể hiện rõ nỗ lực của mọi người.\n\n"
            "1.4K\n\nTrả lời\n\n3 phản hồi\n\n"
            "@river_team\n3 năm trước\nCảnh thứ hai cũng rất ấn tượng.\n\n"
            "1.8K\n\nTrả lời"
        )
        ok, why = MODULE.judge(STRUCTURED_CASE, reply)
        self.assertTrue(ok, why)

    def test_losing_an_opaque_identifier_fails(self) -> None:
        reply = (
            "@atlas_notes\n3 năm trước\nCảnh này rất tuyệt vời.\n\n1.4K\n\n"
            "Trả lời\n\n3 phản hồi\n\n3 năm trước\nCảnh sau cũng ấn tượng.\n\n"
            "1.8K\n\nTrả lời"
        )
        ok, why = MODULE.judge(STRUCTURED_CASE, reply)
        self.assertFalse(ok)
        self.assertIn("opaque", why)

    def test_flattening_a_multiblock_document_fails(self) -> None:
        reply = (
            "@atlas_notes 3 năm trước Cảnh này rất tuyệt vời 1.4K Trả lời "
            "@river_team 3 năm trước Cảnh sau cũng ấn tượng 1.8K Trả lời"
        )
        ok, why = MODULE.judge(STRUCTURED_CASE, reply)
        self.assertFalse(ok)
        self.assertIn("structure", why)

    def test_leaving_source_prose_inside_a_translation_fails(self) -> None:
        reply = (
            "@atlas_notes\n3 năm trước\n이 장면은 Cảnh này rất tuyệt vời.\n\n"
            "1.4K\n\nTrả lời\n\n3 phản hồi\n\n@river_team\n3 năm trước\n"
            "Cảnh sau cũng rất ấn tượng.\n\n1.8K\n\nTrả lời"
        )
        ok, why = MODULE.judge(STRUCTURED_CASE, reply)
        self.assertFalse(ok)
        self.assertIn("Korean", why)


class GateVersionTest(unittest.TestCase):
    def test_the_gate_was_bumped_for_these_checks(self) -> None:
        # Scores only aggregate across samples taken under the same gate; adding
        # checks without bumping this reports a rate for a test never run whole.
        self.assertGreaterEqual(MODULE.GATE_VERSION, 5)


if __name__ == "__main__":
    unittest.main()
