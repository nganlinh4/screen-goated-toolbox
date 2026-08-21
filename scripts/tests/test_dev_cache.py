import subprocess
import tempfile
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


if __name__ == "__main__":
    unittest.main()
