import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = REPO_ROOT / "mobile" / "scripts" / "verify_android_benchmark.py"
SPEC = importlib.util.spec_from_file_location("verify_android_benchmark", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class AndroidBenchmarkVerifierTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.results = self.root / "benchmarkData.json"
        self.contract = self.root / "performance-contract.json"
        self.contract.write_text(
            json.dumps(
                {
                    "android": {
                        "benchmark": {
                            "maxBaselineStartupRatio": 1.05,
                            "maxFrameDurationCpuP95Ms": 32.0,
                            "maxFrameOverrunP95Ms": 8.0,
                            "maxMemoryRssAnonKb": 524288.0,
                            "maxMemoryHeapSizeKb": 393216.0,
                        }
                    }
                }
            ),
            encoding="utf-8",
        )

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def write_results(self, baseline_startup: float = 400.0) -> None:
        def metric(median: float, maximum: float | None = None) -> dict[str, float]:
            result = {"median": median, "P95": median}
            result["maximum"] = median if maximum is None else maximum
            return result

        self.results.write_text(
            json.dumps(
                {
                    "benchmarks": [
                        {
                            "className": "StartupBenchmark",
                            "name": "coldStartupWithBaselineProfile",
                            "metrics": {
                                "timeToInitialDisplayMs": metric(baseline_startup),
                                "frameDurationCpuMs": metric(15.0),
                                "frameOverrunMs": metric(2.0),
                            },
                            "sampledMetrics": {
                                "memoryRssAnonKb": metric(200000.0, 220000.0),
                                "memoryHeapSizeKb": metric(100000.0, 120000.0),
                            },
                        },
                        {
                            "className": "StartupBenchmark",
                            "name": "coldStartupWithoutCompilation",
                            "metrics": {
                                "timeToInitialDisplayMs": metric(500.0),
                            },
                        },
                    ]
                }
            ),
            encoding="utf-8",
        )

    def test_accepts_metrics_and_sampled_metrics(self) -> None:
        self.write_results()
        MODULE.verify([self.results], self.contract, diagnostic_only=False)

    def test_rejects_startup_regression(self) -> None:
        self.write_results(baseline_startup=600.0)
        with self.assertRaisesRegex(ValueError, "maxBaselineStartupRatio"):
            MODULE.verify([self.results], self.contract, diagnostic_only=False)


if __name__ == "__main__":
    unittest.main()
