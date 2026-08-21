import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "package_external_tools.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("package_external_tools", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class PublishedFfmpegFallbackTest(unittest.TestCase):
    def tracked_delivery(self) -> dict:
        return json.loads(
            (REPO / "component-delivery/windows/external-tools-v1.json").read_text(
                encoding="utf-8"
            )
        )

    def test_reviewed_identity_matches_tracked_delivery(self):
        delivered = next(
            item
            for item in self.tracked_delivery()["components"]
            if item["id"] == "ffmpeg-x64"
        )
        self.assertEqual(
            MODULE.expected_ffmpeg_component(),
            MODULE.comparable(delivered),
        )

    def test_changed_delivery_fails_before_network_access(self):
        delivery = copy.deepcopy(self.tracked_delivery())
        delivered = next(
            item for item in delivery["components"] if item["id"] == "ffmpeg-x64"
        )
        delivered["sha256"] = "0" * 64

        with tempfile.TemporaryDirectory() as temporary, mock.patch.object(
            MODULE.urllib.request, "urlopen"
        ) as urlopen:
            with self.assertRaisesRegex(RuntimeError, "reviewed identity"):
                MODULE.verified_published_ffmpeg(Path(temporary), delivery)
        urlopen.assert_not_called()


if __name__ == "__main__":
    unittest.main()
