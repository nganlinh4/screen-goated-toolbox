import subprocess
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
HELPER = REPO / "scripts" / "egui-pinned-checkout.ps1"


def git(*args: str, cwd: Path | None = None) -> str:
    return subprocess.run(
        ["git", *args], cwd=cwd, check=True, capture_output=True, text=True
    ).stdout.strip()


def quoted(value: object) -> str:
    return "'" + str(value).replace("'", "''") + "'"


class PinnedCheckoutTests(unittest.TestCase):
    def setUp(self):
        sandbox = tempfile.TemporaryDirectory()
        self.addCleanup(sandbox.cleanup)
        self.root = Path(sandbox.name)
        self.upstream = self.root / "upstream"
        self.upstream.mkdir()
        git("init", "--quiet", "--initial-branch", "main", cwd=self.upstream)
        git("config", "user.email", "fixture@example.invalid", cwd=self.upstream)
        git("config", "user.name", "Fixture", cwd=self.upstream)
        # Hosted Git serves any reachable commit by id; mirror that locally.
        git("config", "uploadpack.allowReachableSHA1InWant", "true", cwd=self.upstream)
        self.revisions = []
        for index in range(30):
            (self.upstream / "value.txt").write_text(f"{index}\n", encoding="utf-8")
            git("add", "value.txt", cwd=self.upstream)
            git("commit", "--quiet", "-m", f"commit {index}", cwd=self.upstream)
            self.revisions.append(git("rev-parse", "HEAD", cwd=self.upstream))
        self.url = self.upstream.as_uri()
        self.checkout = self.root / "checkout"

    def initialize(self, revision: str) -> subprocess.CompletedProcess:
        command = (
            "$ErrorActionPreference = 'Stop'; "
            f". {quoted(HELPER)}; "
            f"Initialize-PinnedCheckout -Name fixture -Directory {quoted(self.checkout)} "
            f"-Repository {quoted(self.url)} -Revision {quoted(revision)} -Shallow"
        )
        return subprocess.run(
            ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", command],
            capture_output=True,
            text=True,
        )

    def assert_pinned(self, revision: str):
        self.assertEqual(git("-C", str(self.checkout), "rev-parse", "HEAD"), revision)
        self.assertEqual(
            (self.checkout / "value.txt").read_text(encoding="utf-8"),
            f"{self.revisions.index(revision)}\n",
        )

    def test_fresh_shallow_clone_reaches_a_pin_older_than_the_shallow_window(self):
        pinned = self.revisions[0]
        result = self.initialize(pinned)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assert_pinned(pinned)
        self.assertEqual(
            git("-C", str(self.checkout), "rev-parse", "--is-shallow-repository"), "true"
        )

    def test_existing_shallow_checkout_fetches_a_missing_pin(self):
        git("clone", "--quiet", "--depth", "1", self.url, str(self.checkout))
        pinned = self.revisions[3]
        result = self.initialize(pinned)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assert_pinned(pinned)

    def test_unavailable_pin_fails_loudly(self):
        result = self.initialize("0" * 40)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Failed to fetch fixture revision", result.stdout + result.stderr)

    def test_non_git_directory_is_preserved(self):
        self.checkout.mkdir()
        (self.checkout / "work.txt").write_text("keep", encoding="utf-8")
        result = self.initialize(self.revisions[-1])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("is not a Git checkout", result.stdout + result.stderr)
        self.assertEqual((self.checkout / "work.txt").read_text(encoding="utf-8"), "keep")


if __name__ == "__main__":
    unittest.main()
