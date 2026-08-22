import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from build_app_update_manifest import build_payload


class AppUpdateManifestTests(unittest.TestCase):
    def test_payload_binds_version_name_url_size_and_hash(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            installer = Path(directory) / "ScreenGoatedToolbox_v5.5.0.exe"
            installer.write_bytes(b"published installer bytes")
            payload = json.loads(build_payload("5.5.0", installer, "Release notes"))

        self.assertEqual(payload["channel"], "stable")
        self.assertEqual(payload["version"], "5.5.0")
        self.assertEqual(payload["releaseNotes"], "Release notes")
        self.assertEqual(
            payload["installer"],
            {
                "name": installer.name,
                "url": (
                    "https://github.com/nganlinh4/screen-goated-toolbox/releases/"
                    "download/v5.5.0/ScreenGoatedToolbox_v5.5.0.exe"
                ),
                "sizeBytes": len(b"published installer bytes"),
                "sha256": hashlib.sha256(b"published installer bytes").hexdigest(),
            },
        )


if __name__ == "__main__":
    unittest.main()
