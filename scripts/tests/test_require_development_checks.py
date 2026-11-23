import json
import os
from pathlib import Path
import subprocess
import sys
import unittest

from scripts.require_development_checks import require_successful_checks


class DevelopmentGateTests(unittest.TestCase):
    def test_all_selected_jobs_must_match_the_declared_scope(self):
        for required, expected in [("true", "success"), ("false", "skipped")]:
            scope = {"result": "success", "outputs": {"required": required}}
            require_successful_checks({"scope": scope, "first": {"result": expected}, "second": {"result": expected}})
            for status in ["failure", "cancelled", "skipped", "success", "neutral"]:
                if status == expected:
                    continue
                with self.subTest(required=required, status=status), self.assertRaises(ValueError):
                    require_successful_checks({"scope": scope, "first": {"result": expected}, "second": {"result": status}})
            with self.assertRaises(ValueError):
                require_successful_checks({"scope": scope})
        for scope in [{}, {"result": "failure", "outputs": {"required": "true"}},
                      {"result": "success", "outputs": {}},
                      {"result": "success", "outputs": {"required": "unknown"}}]:
            with self.subTest(scope=scope), self.assertRaises(ValueError):
                require_successful_checks({"scope": scope, "job": {"result": "success"}})

    def test_optimized_python_cannot_disable_the_gate(self):
        results = {"scope": {"result": "success", "outputs": {"required": "true"}}, "job": {"result": "failure"}}
        script = Path(__file__).resolve().parents[1] / "require_development_checks.py"
        process = subprocess.run(
            [sys.executable, "-O", str(script)], env=dict(os.environ, RESULTS=json.dumps(results)),
            capture_output=True, check=False,
        )
        self.assertNotEqual(process.returncode, 0)


if __name__ == "__main__":
    unittest.main()
