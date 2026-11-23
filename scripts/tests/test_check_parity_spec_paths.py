"""Parity specs are the contract a port is written from, so a cited file that
moved must fail a check instead of sending a developer to a dead path."""

from pathlib import Path
import tempfile
import unittest

from scripts.check_parity_spec_paths import missing_spec_paths


ROOT = Path(__file__).resolve().parents[2]


class ParitySpecPathTest(unittest.TestCase):
    def test_every_cited_repository_path_exists(self):
        self.assertEqual(missing_spec_paths(ROOT), [])

    def test_a_moved_source_file_is_reported_with_its_spec_line(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / ".claude/parity").mkdir(parents=True)
            (root / "mobile/app/src/full").mkdir(parents=True)
            (root / "mobile/app/src/full/Repo.kt").write_text("", encoding="utf-8")
            (root / "parity-fixtures").mkdir()
            (root / "parity-fixtures/contract.json").write_text("{}", encoding="utf-8")
            (root / ".claude/parity/downloader.md").write_text(
                "\n".join([
                    "- Android: `mobile/app/src/full/Repo.kt`",
                    "- Old: `mobile/app/src/main/Repo.kt:12`",
                    "- Fixture: `parity-fixtures/contract.json`, an elided `mobile/.../Repo.kt`",
                    "- Not a path: `Stream on`, `https://example.test/a.js`, `src/**/*.rs`",
                ]),
                encoding="utf-8",
            )

            self.assertEqual(
                missing_spec_paths(root),
                ["downloader.md:2: mobile/app/src/main/Repo.kt"],
            )


if __name__ == "__main__":
    unittest.main()
