import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from scripts.check_native import discover_crates, needs_egui_checkouts, select_crates


class NativeCheckDiscoveryTests(unittest.TestCase):
    def test_new_packages_are_discovered_but_independent_ignored_checkouts_are_not(self):
        with tempfile.TemporaryDirectory(prefix="sgt-native-discovery-") as directory:
            repo = Path(directory)
            environment = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
            subprocess.run(["git", "init", "--quiet"], cwd=repo, env=environment, check=True)

            def manifest(name):
                path = repo / "native" / name / "Cargo.toml"
                path.parent.mkdir(parents=True)
                path.write_text('[package]\nname = "fixture"\n', encoding="utf-8")
                path.with_name("Cargo.lock").write_text("version = 4\n", encoding="utf-8")
                return path

            tracked = manifest("existing")
            subprocess.run(["git", "add", tracked.relative_to(repo).as_posix()], cwd=repo, env=environment, check=True)
            manifest("new_package")
            manifest("private_checkout")
            (repo / ".gitignore").write_text("native/private_checkout/\n", encoding="utf-8")
            crates = discover_crates(repo)
            self.assertEqual(set(crates), {"existing", "new_package"})
            self.assertEqual(select_crates(crates, None), ["existing", "new_package"])
            self.assertEqual(select_crates(crates, ["new_package", "new_package"]), ["new_package"])
            with self.assertRaises(ValueError):
                select_crates(crates, ["missing"])
            (repo / ".gitignore").write_text("native/private_checkout/\n**/Cargo.lock\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "non-ignored lockfile"):
                discover_crates(repo)
            (repo / ".gitignore").write_text("native/private_checkout/\n", encoding="utf-8")
            tracked.unlink()
            with self.assertRaises(ValueError):
                discover_crates(repo)
            tracked.with_name("Cargo.lock").unlink()
            tracked.parent.rmdir()
            self.assertEqual(set(discover_crates(repo)), {"new_package"})

    def test_empty_selection_and_unreviewed_nested_layout_fail(self):
        with self.assertRaises(ValueError):
            select_crates({}, None)
        with tempfile.TemporaryDirectory(prefix="sgt-native-layout-") as directory:
            repo = Path(directory)
            subprocess.run(["git", "init", "--quiet"], cwd=repo, check=True)
            with self.assertRaises(ValueError):
                discover_crates(repo)
            path = repo / "native" / "nested" / "package" / "Cargo.toml"
            path.parent.mkdir(parents=True)
            path.write_text("[package]\n", encoding="utf-8")
            with self.assertRaises(ValueError):
                discover_crates(repo)

    def test_only_packages_built_from_egui_checkouts_recreate_them(self):
        repo = Path(__file__).resolve().parents[2]
        crates = {"recorder_worker": "native/recorder_worker/Cargo.toml"}
        self.assertFalse(needs_egui_checkouts(repo, crates, ["recorder_worker"]))
        with tempfile.TemporaryDirectory(prefix="sgt-native-egui-") as directory:
            fixture = Path(directory)
            manifest = fixture / "native" / "ui" / "Cargo.toml"
            manifest.parent.mkdir(parents=True)
            manifest.write_text(
                '[dependencies]\negui-snarl = { path = "..\\\\..\\\\libs\\\\egui-snarl" }\n',
                encoding="utf-8",
            )
            crates = {"ui": "native/ui/Cargo.toml"}
            self.assertTrue(needs_egui_checkouts(fixture, crates, ["ui"]))


if __name__ == "__main__":
    unittest.main()
