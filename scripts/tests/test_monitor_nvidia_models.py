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
        self.assertEqual(feed["schemaVersion"], 3)
        self.assertEqual(feed["qualityGateVersion"], MONITOR.quality.GATE_VERSION)

    def test_general_and_vision_models_keep_their_generic_modalities(self) -> None:
        self.assertEqual(MONITOR.published_modality("vendor/general-instruct", False), "text")
        self.assertEqual(
            MONITOR.published_modality("vendor/general-vision", {"passed": True}),
            "vision",
        )


if __name__ == "__main__":
    unittest.main()
