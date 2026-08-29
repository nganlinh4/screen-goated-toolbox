import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "package_creation_windows.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("package_creation_windows", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class CreationArchiveTest(unittest.TestCase):
    def test_inventory_reads_the_canonical_vite_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            runtime = root / "engine.exe"
            runtime.write_bytes(b"engine")
            canonical = root / MODULE.WEB_DIST
            abandoned = root / "3d-generator-ui" / "dist"
            for web_root, content in ((canonical, b"current"), (abandoned, b"stale")):
                for relative in MODULE.WEB_FILES:
                    target = web_root / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(content)

            entries = MODULE.inventory(root, runtime)
            web_entries = dict(entries)

            for relative in MODULE.WEB_FILES:
                self.assertEqual(web_entries[f"web/{relative}"].read_bytes(), b"current")

    def test_archive_is_deterministic_and_has_one_physical_inventory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            engine = root / "engine.exe"
            interface = root / "index.html"
            engine.write_bytes(b"engine")
            interface.write_bytes(b"interface")
            entries = [
                ("bin/sgt_creation_runtime.exe", engine),
                ("web/index.html", interface),
            ]

            first, first_bytes = MODULE.write_archive(root, "1.0.0", entries)
            first.unlink()
            second, second_bytes = MODULE.write_archive(root, "1.0.0", entries)

            self.assertEqual(first.name, second.name)
            self.assertEqual(first_bytes, second_bytes)

    def test_delivery_comparison_ignores_only_local_and_transport_fields(self):
        base = {
            "schemaVersion": 1,
            "hostVersion": "1.0.0",
            "version": "1.0.0",
            "runtimeVersion": "2.0.0",
            "features": ["image_to_3d"],
            "windows": {"asset": "pack.zip", "sha256": "a" * 64},
        }
        candidate = {**base, "windows": {**base["windows"], "assetPath": "local.zip"}}
        delivery = {**base, "windows": {**base["windows"], "downloadUrl": "https://example.invalid/pack.zip"}}

        self.assertEqual(MODULE.comparable(candidate), MODULE.comparable(delivery))


if __name__ == "__main__":
    unittest.main()
