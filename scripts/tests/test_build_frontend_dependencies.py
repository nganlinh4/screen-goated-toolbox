"""Drives build-frontend-dependencies.ps1 against a recording stand-in for npm."""

from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "build-frontend-dependencies.ps1"
PWSH = shutil.which("pwsh")
FAKE_NPM = """@echo off
echo %*>>"%~dp0calls.txt"
if exist node_modules rmdir /s /q node_modules
mkdir node_modules
echo installed>node_modules\\installed.txt
"""


@unittest.skipUnless(PWSH, "PowerShell 7 is required")
class LockfileInstallTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.project = self.root / "frontend"
        self.project.mkdir()
        (self.project / "package.json").write_text('{"name":"fixture"}', encoding="utf-8")
        (self.project / "package-lock.json").write_text('{"lockfileVersion":3}', encoding="utf-8")
        self.npm = self.root / "npm.cmd"
        self.npm.write_text(FAKE_NPM, encoding="ascii")

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def run_helper(self) -> subprocess.CompletedProcess:
        return subprocess.run(
            [PWSH, "-NoProfile", "-File", str(SCRIPT), "-Project", str(self.project), "-Npm", str(self.npm)],
            capture_output=True,
            text=True,
            timeout=120,
        )

    def calls(self) -> list[str]:
        path = self.root / "calls.txt"
        return path.read_text(encoding="ascii").split() if path.exists() else []

    def test_stale_node_modules_is_replaced_by_a_clean_lockfile_install(self):
        (self.project / "node_modules").mkdir()
        (self.project / "node_modules" / "stale.txt").write_text("old", encoding="utf-8")
        result = self.run_helper()
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual(["ci"], self.calls())
        self.assertFalse((self.project / "node_modules" / "stale.txt").exists())

    def test_unchanged_lockfile_skips_reinstall_and_changed_lockfile_reinstalls(self):
        self.assertEqual(0, self.run_helper().returncode)
        self.assertEqual(0, self.run_helper().returncode)
        self.assertEqual(["ci"], self.calls())
        (self.project / "package-lock.json").write_text('{"lockfileVersion":3,"x":1}', encoding="utf-8")
        self.assertEqual(0, self.run_helper().returncode)
        self.assertEqual(["ci", "ci"], self.calls())

    def test_missing_lockfile_fails_instead_of_resolving_fresh_versions(self):
        (self.project / "package-lock.json").unlink()
        result = self.run_helper()
        self.assertNotEqual(0, result.returncode)
        self.assertEqual([], self.calls())


class PackageBuildersTest(unittest.TestCase):
    def test_package_builders_never_reuse_an_unreconciled_install(self):
        release_build = SCRIPT.parents[1] / "build.ps1"
        builders = [*sorted(SCRIPT.parent.glob("build-*.ps1")), release_build]
        for builder in builders:
            text = builder.read_text(encoding="utf-8")
            self.assertNotIn("npm install", text.lower(), builder.name)
            self.assertNotIn("npm.cmd install", text.lower(), builder.name)
            if "npm run build" in text or "npm.cmd run build" in text:
                self.assertIn("build-frontend-dependencies.ps1", text, builder.name)
        release_text = release_build.read_text(encoding="utf-8")
        self.assertEqual(
            release_text.count("npm run build"),
            release_text.count("build-frontend-dependencies.ps1"),
            "every release frontend build must reconcile its lockfile first",
        )


if __name__ == "__main__":
    unittest.main()
