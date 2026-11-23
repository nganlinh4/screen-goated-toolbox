import copy
import json
from pathlib import Path
import sys
import unittest
from unittest import mock

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import model_catalog_realtime  # noqa: E402
from model_catalog_realtime import validate_realtime_engines  # noqa: E402

MANIFEST = json.loads((SCRIPTS.parent / "catalog" / "model_catalog.json").read_text(encoding="utf-8"))


class RealtimeEngineCatalogTest(unittest.TestCase):
    def test_tracked_catalog_describes_every_selector(self):
        validate_realtime_engines(MANIFEST)

    def test_missing_selector_is_rejected(self):
        manifest = copy.deepcopy(MANIFEST)
        del manifest["realtime_transcription_engines"]["google-gemini-2-5-live-transcribe-audio"]
        with self.assertRaisesRegex(ValueError, "every realtime transcription selector"):
            validate_realtime_engines(manifest)

    def test_platform_must_implement_the_engine(self):
        manifest = copy.deepcopy(MANIFEST)
        engines = manifest["realtime_transcription_engines"]
        engines["google-gemini-2-5-live-transcribe-audio"] = {
            "on_device_engine": "demo-engine",
            "platforms": ["windows", "android"],
        }
        with self._only_android_implements("demo-engine"):
            with self.assertRaisesRegex(ValueError, "windows does not implement demo-engine"):
                validate_realtime_engines(manifest)

    def test_unknown_engine_is_rejected(self):
        manifest = copy.deepcopy(MANIFEST)
        engines = manifest["realtime_transcription_engines"]
        engines["google-gemini-2-5-live-transcribe-audio"]["on_device_engine"] = "unknown-engine"
        with self.assertRaisesRegex(ValueError, "unknown on-device engine"):
            validate_realtime_engines(manifest)

    @staticmethod
    def _only_android_implements(engine):
        platforms = {"windows": frozenset(), "android": frozenset({engine})}
        return mock.patch.multiple(
            model_catalog_realtime,
            PLATFORM_ENGINES=platforms,
            ON_DEVICE_ENGINES=frozenset({engine}),
        )


if __name__ == "__main__":
    unittest.main()
