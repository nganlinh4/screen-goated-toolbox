import tempfile
import unittest
from pathlib import Path

from scripts import check_unused_dependencies as unused


def write(path: Path, text: str = "") -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


class CheckUnusedDependenciesTests(unittest.TestCase):
    def setUp(self) -> None:
        self.scratch = tempfile.TemporaryDirectory()
        self.repo = Path(self.scratch.name) / "repo"
        write(self.repo / "Cargo.toml", "[package]\nname = 'root'\n")
        write(self.repo / "src/main.rs", "mod own;\n")
        write(self.repo / "src/own.rs", "use flate2::read::GzDecoder;\n")
        write(self.repo / "src/shared/mod.rs", "mod child;\n")
        write(self.repo / "src/shared/child.rs", "use zip::ZipArchive;\n")
        write(self.repo / "src/single.rs", "use open::that;\n")
        write(self.repo / "src/single/inner.rs", "use url::Url;\n")
        write(self.repo / "src/unused.rs", "use rand::random;\n")
        self.worker = self.repo / "native/worker"
        write(self.worker / "Cargo.toml", "[package]\nname = 'worker'\n")
        write(
            self.worker / "src/main.rs",
            '#[path = "../../../src/shared/mod.rs"]\nmod shared;\n'
            '#[path = "../../../src/single.rs"]\nmod single;\n',
        )

    def tearDown(self) -> None:
        self.scratch.cleanup()

    def relative(self, paths: list[Path]) -> set[str]:
        return {path.resolve().relative_to(self.repo.resolve()).as_posix() for path in paths}

    def test_path_modules_outside_the_crate_are_followed_with_their_children(self) -> None:
        sources = unused.crate_sources(self.worker, self.repo)
        external = unused.external_modules(sources, {path.resolve() for path in sources})
        self.assertEqual(
            self.relative(external),
            {"src/shared/mod.rs", "src/shared/child.rs", "src/single.rs", "src/single/inner.rs"},
        )

    def test_mirror_keeps_the_crate_layout_and_places_shared_modules_under_src(self) -> None:
        destination = Path(self.scratch.name) / "mirrors"
        mirror = unused.build_mirror(self.worker, self.repo, destination)
        self.assertTrue((mirror / "Cargo.toml").is_file())
        self.assertTrue((mirror / "src/main.rs").is_file())
        shared = mirror / "src/sgt_shared_modules/src"
        self.assertTrue((shared / "shared/child.rs").is_file())
        self.assertTrue((shared / "single/inner.rs").is_file())
        self.assertFalse((shared / "unused.rs").exists())

    def test_root_crate_owns_only_its_sources_and_skips_nested_crates(self) -> None:
        sources = self.relative(unused.crate_sources(self.repo, self.repo))
        self.assertIn("src/own.rs", sources)
        self.assertFalse(any(path.startswith("native/") for path in sources))
        self.assertEqual(
            unused.cargo_crates(self.repo), [self.repo, self.worker]
        )

    def test_npm_packages_are_the_locked_top_level_projects(self) -> None:
        write(self.repo / "ui/package.json", "{}")
        write(self.repo / "ui/package-lock.json", "{}")
        write(self.repo / "unlocked/package.json", "{}")
        self.assertEqual(unused.npm_packages(self.repo), [self.repo / "ui"])

    def test_a_missing_cargo_machete_names_the_install_command(self) -> None:
        with self.assertRaises(SystemExit) as raised:
            unused.check_cargo(self.repo, "sgt-no-such-cargo-machete")
        self.assertIn("cargo install cargo-machete --version 0.9.2", str(raised.exception))


if __name__ == "__main__":
    unittest.main()
