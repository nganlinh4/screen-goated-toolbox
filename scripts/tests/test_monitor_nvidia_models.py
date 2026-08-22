import importlib.util
import sys
import unittest
from pathlib import Path
from unittest import mock


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
                    "gate": MONITOR.AVAILABILITY_GATE_VERSION,
                    "answered": 3,
                    "attempts": 3,
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
        self.assertEqual(feed["schemaVersion"], 3)
        self.assertEqual(
            feed["availabilityGateVersion"], MONITOR.AVAILABILITY_GATE_VERSION
        )

    def test_general_and_vision_models_keep_their_generic_modalities(self) -> None:
        self.assertEqual(MONITOR.published_modality("vendor/general-instruct", False), "text")
        self.assertEqual(
            MONITOR.published_modality("vendor/general-vision", {"passed": True}),
            "vision",
        )


class EligibilityGateTests(unittest.TestCase):
    @staticmethod
    def sample(answered: int = 3, attempts: int = 3, p50_ms: int = 400) -> dict:
        return {"answered": answered, "attempts": attempts, "p50_ms": p50_ms}

    def test_task_specific_diagnostic_never_changes_operational_health(self) -> None:
        sample = self.sample() | {"translation_diagnostic_passed": False}
        self.assertTrue(MONITOR.healthy(sample))

    def test_one_transient_miss_is_tolerated(self) -> None:
        self.assertTrue(MONITOR.healthy(self.sample(answered=2)))

    def test_too_few_replies_or_unusable_latency_is_unhealthy(self) -> None:
        self.assertFalse(MONITOR.healthy(self.sample(answered=1)))
        self.assertFalse(MONITOR.healthy(self.sample(p50_ms=6_001)))

    def test_old_preset_verdict_is_migrated_from_transport_evidence(self) -> None:
        known = {
            "eligibility_gate": 5,
            "healthy_streak": 20,
            "failing_streak": 0,
            "eligible": False,
            "recent": [
                self.sample() | {"passed": False},
                self.sample() | {"passed": False},
            ],
        }
        self.assertEqual(
            MONITOR.updated_eligibility(known, self.sample()),
            (3, 0, True),
        )

    def test_three_passes_under_the_same_gate_promote(self) -> None:
        known = {
            "eligibility_gate": MONITOR.AVAILABILITY_GATE_VERSION,
            "healthy_streak": 2,
            "failing_streak": 0,
            "eligible": False,
        }
        self.assertEqual(
            MONITOR.updated_eligibility(known, self.sample()),
            (3, 0, True),
        )

    def test_public_success_rate_means_availability_not_task_correctness(self) -> None:
        state = {
            "eligible": True,
            "control": "plain",
            "vision": False,
            "recent": [
                self.sample() | {"passed": False, "translation_diagnostic_passed": False},
                self.sample(answered=2) | {"passed": False},
            ],
        }
        feed = MONITOR.published(
            {"models": {"nvidia/general": state}}, "2026-08-22T00:00:00Z"
        )
        self.assertEqual(feed["models"][0]["success_rate"], 1.0)

    @mock.patch.object(MONITOR, "measure_vision", return_value=None)
    @mock.patch.object(MONITOR, "measure")
    def test_focused_probe_preserves_unselected_history(self, measure, _vision) -> None:
        measure.return_value = self.sample() | {
            "gate": MONITOR.AVAILABILITY_GATE_VERSION,
            "passed": True,
            "reason": "",
            "p95_ms": 400,
        }
        history = {
            "models": {
                "nvidia/selected": {"control": "plain", "recent": []},
                "nvidia/untouched": {"control": "plain", "recent": [self.sample()]},
            }
        }
        updated = MONITOR.run(history, "key", None, ["nvidia/selected"])
        self.assertIn("nvidia/untouched", updated["models"])


if __name__ == "__main__":
    unittest.main()
