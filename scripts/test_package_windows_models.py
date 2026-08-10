import tempfile
import unittest
from pathlib import Path

from package_windows_models import PACKAGES, cleanup_stale_staging


class StagingCleanupTests(unittest.TestCase):
    def test_tempfile_suffix_with_underscore_is_cleaned(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            staging = root / f"{PACKAGES[0].component_id}-abc_1234"
            staging.mkdir()
            (staging / "package.zip").write_bytes(b"partial")

            cleanup_stale_staging(root)

            self.assertFalse(staging.exists())

    def test_hostile_suffix_and_contents_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            hostile_name = root / f"{PACKAGES[0].component_id}-abc-1234"
            hostile_name.mkdir()
            (hostile_name / "user.txt").write_text("keep", encoding="utf-8")
            cleanup_stale_staging(root)
            self.assertEqual(
                (hostile_name / "user.txt").read_text(encoding="utf-8"), "keep"
            )

            owned_name = root / f"{PACKAGES[0].component_id}-abc_1234"
            owned_name.mkdir()
            (owned_name / "user.txt").write_text("keep", encoding="utf-8")
            with self.assertRaises(ValueError):
                cleanup_stale_staging(root)
            self.assertEqual(
                (owned_name / "user.txt").read_text(encoding="utf-8"), "keep"
            )


if __name__ == "__main__":
    unittest.main()
