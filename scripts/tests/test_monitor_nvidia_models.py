import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location(
    "monitor_nvidia_models", SCRIPTS / "monitor_nvidia_models.py"
)
MONITOR = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MONITOR)


class NvidiaModelCapabilityTests(unittest.TestCase):
    def test_dedicated_translator_is_not_published_as_generic_text(self) -> None:
        self.assertIsNone(
            MONITOR.published_modality("vendor/translate-specialist", False),
        )

    def test_dedicated_translator_is_omitted_from_the_generic_feed(self) -> None:
        state = {
            "eligible": True,
            "control": "plain",
            "vision": False,
            "recent": [
                {
                    "gate": MONITOR.quality.GATE_VERSION,
                    "passed": True,
                    "p50_ms": 100,
                }
            ],
        }
        feed = MONITOR.published(
            {"models": {"vendor/translate-specialist": state}},
            "2026-08-22T00:00:00Z",
        )
        self.assertEqual(feed["models"], [])
        self.assertEqual(feed["schemaVersion"], 2)
        self.assertEqual(feed["qualityGateVersion"], MONITOR.quality.GATE_VERSION)

    def test_general_and_vision_models_keep_their_generic_modalities(self) -> None:
        self.assertEqual(MONITOR.published_modality("vendor/general-instruct", False), "text")
        self.assertEqual(
            MONITOR.published_modality("vendor/general-vision", {"passed": True}),
            "vision",
        )


class EligibilityGateTests(unittest.TestCase):
    def test_a_pass_under_a_new_gate_cannot_inherit_old_eligibility(self) -> None:
        known = {
            "eligibility_gate": MONITOR.quality.GATE_VERSION - 1,
            "healthy_streak": 20,
            "failing_streak": 0,
            "eligible": True,
        }
        self.assertEqual(
            MONITOR.updated_eligibility(known, {"passed": True}),
            (1, 0, False),
        )

    def test_three_passes_under_the_same_gate_promote(self) -> None:
        known = {
            "eligibility_gate": MONITOR.quality.GATE_VERSION,
            "healthy_streak": 2,
            "failing_streak": 0,
            "eligible": False,
        }
        self.assertEqual(
            MONITOR.updated_eligibility(known, {"passed": True}),
            (3, 0, True),
        )


if __name__ == "__main__":
    unittest.main()
