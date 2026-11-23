import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from scripts.development_check_scope import requires_checks


class DevelopmentCheckScopeTests(unittest.TestCase):
    def test_committed_docs_skip_but_source_removal_and_missing_history_do_not(self):
        with tempfile.TemporaryDirectory(prefix="sgt-check-scope-") as directory:
            repo = Path(directory)
            environment = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
            hooks = repo / "empty-hooks"
            hooks.mkdir()

            def git(*args, capture=False):
                result = subprocess.run(
                    ["git", "-c", "user.name=Development tests", "-c", "user.email=ci@example.invalid",
                     "-c", f"core.hooksPath={hooks}", *args],
                    cwd=repo, env=environment, check=True, text=True, capture_output=capture,
                )
                return result.stdout.strip() if capture else None

            def commit():
                git("add", ".")
                git("commit", "--quiet", "-m", "fixture")
                return git("rev-parse", "HEAD", capture=True)

            git("init", "--quiet", "--initial-branch=main")
            (repo / "source.rs").write_text("source", encoding="utf-8")
            base = commit()
            (repo / "README.md").write_text("docs", encoding="utf-8")
            docs = commit()
            self.assertFalse(requires_checks("push", {"before": base, "after": docs}, repo))
            (repo / "source.rs").rename(repo / "AGENTS.md")
            renamed = commit()
            self.assertTrue(requires_checks("push", {"before": docs, "after": renamed}, repo))
            self.assertTrue(requires_checks("pull_request", {"pull_request": {
                "base": {"sha": docs}, "head": {"sha": renamed},
            }}, repo))
            with self.assertRaises(RuntimeError):
                requires_checks("push", {"before": "1" * 40, "after": renamed}, repo)

    def test_explicit_and_initial_runs_require_checks_and_bad_identities_fail(self):
        self.assertTrue(requires_checks("workflow_dispatch", {}, Path.cwd()))
        self.assertTrue(requires_checks("push", {"before": "0" * 40, "after": "1" * 40}, Path.cwd()))
        self.assertFalse(requires_checks("push", {"before": "1" * 40, "after": "0" * 40, "deleted": True}, Path.cwd()))
        with self.assertRaises(ValueError):
            requires_checks("push", {"before": "1" * 40, "after": "2" * 40, "deleted": True}, Path.cwd())
        with self.assertRaises(ValueError):
            requires_checks("push", {"before": "HEAD", "after": "1" * 40}, Path.cwd())
        with self.assertRaises(ValueError):
            requires_checks("push", {"before": "1" * 41, "after": "2" * 40}, Path.cwd())


if __name__ == "__main__":
    unittest.main()
