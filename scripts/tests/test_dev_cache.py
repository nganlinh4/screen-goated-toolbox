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


if __name__ == "__main__":
    unittest.main()
