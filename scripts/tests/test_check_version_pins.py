"""The version-pin checker is what a release trusts instead of building twice.

The failing case is the one worth testing: a bump that moved `Cargo.toml` and
left a manifest behind must be reported, not passed over.
"""

import importlib.util
from pathlib import Path
import sys
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "check_version_pins.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("check_version_pins", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class CrateVersionTest(unittest.TestCase):
    def test_reads_the_package_version_not_a_dependency_pin(self) -> None:
        version = MODULE.crate_version()
        self.assertRegex(version, r"^\d+\.\d+\.\d+")


class ManifestDiscoveryTest(unittest.TestCase):
    def test_finds_every_shipped_host_pin(self) -> None:
        # Discovery, not a hardcoded list: a manifest added later must be covered
        # without editing this script.
        found = {path.name for path in MODULE.host_version_manifests()}
        self.assertIn("creation-runtime-v1.json", found)
        self.assertIn("external-tools-v1.json", found)


class HostBoundCrateDiscoveryTest(unittest.TestCase):
    def test_finds_recorder_worker_without_a_hardcoded_path(self) -> None:
        found = {
            path.relative_to(MODULE.REPO).as_posix()
            for path in MODULE.host_bound_cargo_manifests()
        }
        self.assertIn("native/recorder_worker/Cargo.toml", found)


class AppResourceTest(unittest.TestCase):
    def test_covers_both_spellings_of_the_version(self) -> None:
        # The installer shows whichever spelling is stale, so all four are pinned.
        expected = [text for _, text in MODULE.app_rc_expectations("5.5.0")]
        self.assertEqual(
            expected,
            [
                "FILEVERSION 5,5,0,0",
                "PRODUCTVERSION 5,5,0,0",
                'VALUE "FileVersion", "5.5.0.0"',
                'VALUE "ProductVersion", "5.5.0.0"',
            ],
        )


class ShippedTreeTest(unittest.TestCase):
    def test_the_repository_is_currently_consistent(self) -> None:
        # Guards the release directly: if a bump lands without the manifests,
        # this fails here rather than partway through a Windows build.
        with mock.patch.object(sys, "argv", ["check_version_pins.py"]):
            self.assertEqual(MODULE.main(), 0, "run scripts/check_version_pins.py --write")


if __name__ == "__main__":
    unittest.main()
