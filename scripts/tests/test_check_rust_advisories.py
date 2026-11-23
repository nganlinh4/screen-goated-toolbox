import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

from scripts.check_rust_advisories import scan


class RustAdvisoryReportTests(unittest.TestCase):
    def test_clean_and_vulnerable_inventories_preserve_the_result(self):
        for count in (0, 1):
            report = {"vulnerabilities": {"count": count, "list": [{}] * count}}
            completed = subprocess.CompletedProcess([], count, json.dumps(report), "")
            with patch("scripts.check_rust_advisories.subprocess.run", return_value=completed):
                self.assertEqual(scan("scanner", Path.cwd(), "Cargo.lock", "database", True), report)

    def test_tool_errors_missing_inventory_and_false_green_statuses_fail(self):
        for status, output in (
            (2, ""), (1, ""), (0, "{}"),
            (0, '{"vulnerabilities":{"count":1,"list":[{}]}}'),
            (1, '{"vulnerabilities":{"count":0,"list":[]}}'),
            (0, '{"vulnerabilities":{"count":0,"list":[{}]}}'),
        ):
            with self.subTest(status=status, output=output):
                completed = subprocess.CompletedProcess([], status, output, "fixture failure")
                with patch("scripts.check_rust_advisories.subprocess.run", return_value=completed):
                    with self.assertRaises(RuntimeError):
                        scan("scanner", Path.cwd(), "Cargo.lock", "database", False)


if __name__ == "__main__":
    unittest.main()
