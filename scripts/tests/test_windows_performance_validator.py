import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
VALIDATOR = REPO_ROOT / "scripts" / "validate-windows-performance-report.ps1"


class WindowsPerformanceValidatorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.powershell = shutil.which("pwsh") or shutil.which("powershell")
        if self.powershell is None:
            self.skipTest("PowerShell is unavailable")
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.report = self.root / "report.json"
        self.contract = self.root / "contract.json"
        self.contract.write_text(
            json.dumps(
                {
                    "windows": {
                        "maxShippingBinaryBytes": 150,
                        "maxBalancedSizeRatio": 1.1,
                        "maxPerfSizeRatio": 1.2,
                        "maxBalancedLatencyRatio": 1.1,
                        "maxPerfLatencyRatio": 1.1,
                        "requiredSmokes": ["result"],
                        "absoluteBudgets": {
                            "result": {
                                "maxMedianElapsedMs": 1000,
                                "maxPeakWorkingSetBytes": 1000,
                                "maxPeakThreads": 10,
                                "maxPeakProcessCount": 3,
                            }
                        },
                    }
                }
            ),
            encoding="utf-8",
        )

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def write_report(self, compact_bytes: int = 100) -> None:
        measurements = []
        for build, elapsed in (("compact", 100), ("balanced", 90), ("perf", 95)):
            measurements.append(
                {
                    "build": build,
                    "smoke": "result",
                    "elapsedMs": elapsed,
                    "peakWorkingSetBytes": 500,
                    "peakThreads": 5,
                    "peakProcessCount": 2,
                }
            )
        self.report.write_text(
            json.dumps(
                {
                    "schemaVersion": 1,
                    "artifacts": [
                        {"build": "compact", "bytes": compact_bytes},
                        {"build": "balanced", "bytes": 120},
                        {"build": "perf", "bytes": 105},
                    ],
                    "measurements": measurements,
                }
            ),
            encoding="utf-8",
        )

    def run_validator(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                self.powershell,
                "-NoProfile",
                "-File",
                str(VALIDATOR),
                "-Report",
                str(self.report),
                "-Contract",
                str(self.contract),
                "-ExpectedRuns",
                "1",
            ],
            check=False,
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
        )

    def test_candidate_rejection_does_not_fail_shipping(self) -> None:
        self.write_report()
        result = self.run_validator()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        validation = json.loads(self.report.read_text(encoding="utf-8-sig"))["validation"]
        self.assertTrue(validation["shipping"]["eligible"])
        self.assertFalse(validation["candidates"]["balanced"]["eligible"])
        self.assertTrue(validation["candidates"]["perf"]["eligible"])

    def test_shipping_budget_failure_is_fatal(self) -> None:
        self.write_report(compact_bytes=200)
        result = self.run_validator()
        self.assertNotEqual(result.returncode, 0)
        validation = json.loads(self.report.read_text(encoding="utf-8-sig"))["validation"]
        self.assertFalse(validation["shipping"]["eligible"])


if __name__ == "__main__":
    unittest.main()
