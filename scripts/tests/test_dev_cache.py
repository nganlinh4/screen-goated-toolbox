import os
import subprocess
import tempfile
import time
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "dev-cache.ps1"


class DevCacheTests(unittest.TestCase):
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
                    str(SCRIPT),
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
                    str(SCRIPT),
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
                    str(SCRIPT),
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
                    str(SCRIPT),
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


if __name__ == "__main__":
    unittest.main()
