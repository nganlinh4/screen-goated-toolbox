from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "package_recorder_components",
    REPO / "scripts" / "package_recorder_components.py",
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RecorderPackageContractTests(unittest.TestCase):
    def test_package_is_deterministic_and_content_addressed(self) -> None:
        with tempfile.TemporaryDirectory() as raw_temp:
            temp = Path(raw_temp)
            output = temp / "output"
            output.mkdir()
            source = temp / "index.js"
            source.write_text("console.log('stable');\n", encoding="utf-8")
            files = [("assets/index.js", source)]

            first = MODULE.package(REPO, output, "recorder-web", "1.0.0", files)
            second = MODULE.package(REPO, output, "recorder-web", "1.0.0", files)

            self.assertEqual(first["asset"], second["asset"])
            self.assertEqual(first["sha256"], second["sha256"])
            self.assertTrue((output / str(first["asset"])).is_file())

    def test_unchanged_payload_reuses_verified_asset_name(self) -> None:
        current = {
            "id": "recorder-web",
            "version": "5.5.0",
            "asset": "recorder-web-5.5.0-aaaaaaaaaaaaaaaa.zip",
            "assetPath": "generated.zip",
            "sizeBytes": 12,
            "sha256": "a" * 64,
            "unpackedSizeBytes": 34,
            "files": [{"path": "index.html", "sizeBytes": 34, "sha256": "b" * 64}],
        }
        verified = dict(current)
        verified.update(
            version="5.4.3",
            asset="recorder-web-5.4.3-aaaaaaaaaaaaaaaa.zip",
            downloadUrl="https://example.invalid/recorder.zip",
        )

        MODULE.reuse_verified_asset_names(
            {"components": [current]}, {"components": [verified]}
        )

        self.assertEqual(verified["asset"], current["asset"])
        self.assertEqual("5.5.0", current["version"])

    def test_package_rejects_unsafe_inventory_paths(self) -> None:
        with tempfile.TemporaryDirectory() as raw_temp:
            temp = Path(raw_temp)
            output = temp / "output"
            output.mkdir()
            source = temp / "file.txt"
            source.write_text("data", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "unsafe recorder package entry"):
                MODULE.package(
                    REPO,
                    output,
                    "recorder-web",
                    "1.0.0",
                    [("../escape.txt", source)],
                )

    def test_build_script_defaults_to_managed_package_lanes(self) -> None:
        script = (REPO / "scripts" / "build-recorder-component-packs.ps1").read_text(
            encoding="utf-8",
        )
        self.assertIn('"cargo\\package"', script)
        self.assertIn('"packages\\jobs\\recorder"', script)
        self.assertNotIn('"local-runtime-bundles\\sgt_recorder"', script)
        self.assertNotIn('"native\\recorder_worker\\target"', script)


if __name__ == "__main__":
    unittest.main()
