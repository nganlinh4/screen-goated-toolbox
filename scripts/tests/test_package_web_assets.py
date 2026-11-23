import copy
import importlib.util
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "package_web_assets.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("package_web_assets", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def component(asset: str, sha256: str = "a" * 64) -> dict:
    return {
        "id": "example-web",
        "asset": asset,
        "assetPath": "generated.zip",
        "sizeBytes": 12,
        "sha256": sha256,
        "unpackedSizeBytes": 34,
        "files": [{"path": "index.html", "sizeBytes": 34, "sha256": "b" * 64}],
    }


class VerifiedAssetReuseTest(unittest.TestCase):
    def test_reuses_prior_name_for_identical_payload(self):
        current = component("example-web-5.5.0-aaaaaaaaaaaaaaaa.zip")
        verified = component("example-web-5.4.3-aaaaaaaaaaaaaaaa.zip")
        descriptor = {"version": "5.5.0", "windows": {"components": [current]}}
        delivery = {"version": "5.4.3", "windows": {"components": [verified]}}

        MODULE.reuse_verified_asset_names(descriptor, delivery)

        self.assertEqual(verified["asset"], current["asset"])
        self.assertEqual("generated.zip", current["assetPath"])
        self.assertEqual("5.4.3", descriptor["version"])

    def test_keeps_new_name_when_payload_changed(self):
        current = component("example-web-5.5.0-cccccccccccccccc.zip", "c" * 64)
        verified = component("example-web-5.4.3-aaaaaaaaaaaaaaaa.zip")
        original = copy.deepcopy(current)

        descriptor = {"version": "5.5.0", "windows": {"components": [current]}}
        MODULE.reuse_verified_asset_names(
            descriptor,
            {"version": "5.4.3", "windows": {"components": [verified]}},
        )

        self.assertEqual(original, current)
        self.assertEqual("5.5.0", descriptor["version"])


class OutputDirectoryTest(unittest.TestCase):
    def test_default_output_is_a_managed_cache_job_not_the_checkout(self):
        with tempfile.TemporaryDirectory() as cache, tempfile.TemporaryDirectory() as repo:
            with mock.patch.dict(os.environ, {"SGT_DEV_CACHE_ROOT": cache}):
                output = MODULE.resolve_output_dir(Path(repo), None)
            jobs = Path(cache).resolve() / "packages" / "jobs" / "web-assets"
            self.assertEqual(jobs, output.parent)
            self.assertFalse(output.is_relative_to(Path(repo).resolve()))

    def test_explicit_output_is_kept_inside_allowed_roots(self):
        with tempfile.TemporaryDirectory() as cache, tempfile.TemporaryDirectory() as repo:
            explicit = Path(cache) / "packages" / "release" / "sgt_web_assets"
            with mock.patch.dict(os.environ, {"SGT_DEV_CACHE_ROOT": cache}):
                self.assertEqual(
                    explicit.resolve(), MODULE.resolve_output_dir(Path(repo), str(explicit))
                )
                with self.assertRaises(ValueError):
                    MODULE.resolve_output_dir(Path(repo), str(Path(cache) / "elsewhere"))


if __name__ == "__main__":
    unittest.main()
