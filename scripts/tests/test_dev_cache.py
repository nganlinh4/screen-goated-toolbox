import os
import subprocess
import tempfile
import time
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "dev-cache.ps1"


class DevCacheTests(unittest.TestCase):
    def setUp(self):
        self.sandbox = tempfile.TemporaryDirectory()
        self.addCleanup(self.sandbox.cleanup)
        self.checkout = Path(self.sandbox.name) / "checkout"
        self.script = self.checkout / "scripts" / SCRIPT.name
        self.script.parent.mkdir(parents=True)
        # Exercise the unchanged CLI with a simulated checkout so test cache
        # roots remain outside its protected source tree within the sandbox.
        self.script.write_bytes(SCRIPT.read_bytes())

    def test_cache_root_inside_checkout_is_rejected_before_creation(self):
        for root in [self.checkout, self.checkout / "cache"]:
            with self.subTest(root=root):
                result = subprocess.run(
                    [
                        "powershell", "-NoProfile", "-ExecutionPolicy", "Bypass",
                        "-File", str(self.script), "-Action", "Path",
                        "-CacheRoot", str(root),
                    ],
                    capture_output=True,
                    text=True,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("outside the repository", result.stderr)
        self.assertFalse((self.checkout / "cache").exists())
        self.assertEqual(self.script.read_bytes(), SCRIPT.read_bytes())

    def test_package_lane_protects_release_checkpoints(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "sgt-cache"
            checkpoint = root / "packages" / "release" / "component" / "manifest.json"
            checkpoint.parent.mkdir(parents=True)
            checkpoint.write_text("{}", encoding="utf-8")

            subprocess.run(
                [
                    "powershell",
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(self.script),
                    "-Action",
                    "Prune",
                    "-CacheRoot",
                    str(root),
                    "-MaxGiB",
                    "5",
                    "-InactiveDays",
                    "1",
                    "-ProtectLane",
                    "package",
                    "-Apply",
                ],
                check=True,
                capture_output=True,
                text=True,
            )

            self.assertTrue(checkpoint.is_file())

    def test_promotion_records_are_never_cache_candidates(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "sgt-cache"
            records = [
                root / "promotion" / "component.json",
                root / "packages" / "promoted" / "component.json",
            ]
            old_time = time.time() - (3 * 24 * 60 * 60)
            for record in records:
                record.parent.mkdir(parents=True)
                record.write_text("{}", encoding="utf-8")
                os.utime(record, (old_time, old_time))

            subprocess.run(
                [
                    "powershell",
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(self.script),
                    "-Action",
                    "Prune",
                    "-CacheRoot",
                    str(root),
                    "-MaxGiB",
                    "5",
                    "-InactiveDays",
                    "1",
                    "-Apply",
                ],
                check=True,
                capture_output=True,
                text=True,
            )

            for record in records:
                self.assertTrue(record.is_file())

    def test_inactive_custom_cargo_lane_is_pruned(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "sgt-cache"
            custom_artifact = root / "cargo" / "old-validation" / "debug" / "artifact.bin"
            custom_artifact.parent.mkdir(parents=True)
            custom_artifact.write_bytes(b"old")
            old_time = time.time() - (3 * 24 * 60 * 60)
            os.utime(custom_artifact, (old_time, old_time))

            subprocess.run(
                [
                    "powershell",
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(self.script),
                    "-Action",
                    "Prune",
                    "-CacheRoot",
                    str(root),
                    "-MaxGiB",
                    "5",
                    "-InactiveDays",
                    "1",
                    "-ProtectLane",
                    "dev",
                    "-Apply",
                ],
                check=True,
                capture_output=True,
                text=True,
            )

            self.assertFalse(custom_artifact.parent.parent.exists())

    def test_inactive_direct_package_and_legacy_lane_are_pruned(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "sgt-cache"
            artifacts = [
                root / "packages" / "old-component" / "candidate.zip",
                root / "release-test" / "debug" / "app.exe",
            ]
            old_time = time.time() - (3 * 24 * 60 * 60)
            for artifact in artifacts:
                artifact.parent.mkdir(parents=True)
                artifact.write_bytes(b"old")
                os.utime(artifact, (old_time, old_time))

            subprocess.run(
                [
                    "powershell",
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(self.script),
                    "-Action",
                    "Prune",
                    "-CacheRoot",
                    str(root),
                    "-MaxGiB",
                    "5",
                    "-InactiveDays",
                    "1",
                    "-Apply",
                ],
                check=True,
                capture_output=True,
                text=True,
            )

            self.assertFalse(artifacts[0].parent.exists())
            self.assertFalse((root / "release-test").exists())

    def prune(self, root, *extra):
        subprocess.run(
            [
                "powershell", "-NoProfile", "-ExecutionPolicy", "Bypass",
                "-File", str(self.script), "-Action", "Prune",
                "-CacheRoot", str(root), "-MaxGiB", "5", "-InactiveDays", "1",
                *extra, "-Apply",
            ],
            check=True,
            capture_output=True,
            text=True,
        )

    def test_inactive_runtime_state_roots_are_pruned(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "sgt-cache"
            old_state = root / "runtime" / "old-test" / "local-app-data" / "state.json"
            new_state = root / "runtime" / "new-test" / "local-app-data" / "state.json"
            for state in [old_state, new_state]:
                state.parent.mkdir(parents=True)
                state.write_text("{}", encoding="utf-8")
            old_time = time.time() - (3 * 24 * 60 * 60)
            os.utime(old_state, (old_time, old_time))

            self.prune(root, "-ProtectLane", "dev")

            self.assertFalse((root / "runtime" / "old-test").exists())
            self.assertTrue(new_state.is_file())

    def test_superseded_linked_executables_leave_with_their_databases(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "sgt-cache"
            deps = root / "cargo" / "dev" / "debug" / "deps"
            deps.mkdir(parents=True)
            library = deps / "libtool-0123456789abcdef.rlib"
            library.write_bytes(b"lib")
            now = time.time()
            names = [f"app-{index:016x}" for index in range(3)]
            for age, name in enumerate(names):
                for suffix in [".exe", ".pdb"]:
                    artifact = deps / f"{name}{suffix}"
                    artifact.write_bytes(b"bin")
                    os.utime(artifact, (now - age * 60, now - age * 60))

            self.prune(root, "-ProtectLane", "dev")

            for name in names[:2]:
                self.assertTrue((deps / f"{name}.exe").is_file())
                self.assertTrue((deps / f"{name}.pdb").is_file())
            self.assertFalse((deps / f"{names[2]}.exe").exists())
            self.assertFalse((deps / f"{names[2]}.pdb").exists())
            self.assertTrue(library.is_file())

    def test_orphaned_codegen_objects_are_pruned_once_a_day_old(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "sgt-cache"
            deps = root / "cargo" / "dev" / "debug" / "deps"
            deps.mkdir(parents=True)
            stale = deps / "app-0123456789abcdef.cgu0.rcgu.o"
            fresh = deps / "app-0123456789abcdef.cgu1.rcgu.o"
            for artifact in [stale, fresh]:
                artifact.write_bytes(b"obj")
            old_time = time.time() - (2 * 24 * 60 * 60)
            os.utime(stale, (old_time, old_time))

            self.prune(root, "-ProtectLane", "dev")

            self.assertFalse(stale.exists())
            self.assertTrue(fresh.is_file())


if __name__ == "__main__":
    unittest.main()
