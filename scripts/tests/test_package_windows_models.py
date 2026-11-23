import tempfile
import unittest
from pathlib import Path

from scripts.package_windows_models import PACKAGES, PackageSpec, cleanup_stale_staging

FIXTURE = PackageSpec(
    component_id="example-model",
    version="1",
    source_filename="example.tar.bz2",
    source_url="https://example.invalid/example.tar.bz2",
    source_size=1,
    source_sha256="0" * 64,
    top_directory="example",
)


class StagingCleanupTests(unittest.TestCase):
    def test_no_windows_model_is_packaged(self):
        self.assertEqual(PACKAGES, ())

    def test_tempfile_suffix_with_underscore_is_cleaned(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            staging = root / f"{FIXTURE.component_id}-abc_1234"
            staging.mkdir()
            (staging / "package.zip").write_bytes(b"partial")

            cleanup_stale_staging(root, (FIXTURE,))

            self.assertFalse(staging.exists())

    def test_unlisted_component_staging_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            staging = root / f"{FIXTURE.component_id}-abc_1234"
            staging.mkdir()
            (staging / "package.zip").write_bytes(b"partial")

            cleanup_stale_staging(root)

            self.assertTrue((staging / "package.zip").exists())

    def test_hostile_suffix_and_contents_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            hostile_name = root / f"{FIXTURE.component_id}-abc-1234"
            hostile_name.mkdir()
            (hostile_name / "user.txt").write_text("keep", encoding="utf-8")
            cleanup_stale_staging(root, (FIXTURE,))
            self.assertEqual(
                (hostile_name / "user.txt").read_text(encoding="utf-8"), "keep"
            )

            owned_name = root / f"{FIXTURE.component_id}-abc_1234"
            owned_name.mkdir()
            (owned_name / "user.txt").write_text("keep", encoding="utf-8")
            with self.assertRaises(ValueError):
                cleanup_stale_staging(root, (FIXTURE,))
            self.assertEqual(
                (owned_name / "user.txt").read_text(encoding="utf-8"), "keep"
            )


if __name__ == "__main__":
    unittest.main()
