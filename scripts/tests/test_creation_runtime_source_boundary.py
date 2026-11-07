import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class CreationRuntimeSourceBoundaryTests(unittest.TestCase):
    def test_runtime_source_is_configured_never_hardcoded(self) -> None:
        for relative in ("scripts/build-creation-windows-pack.ps1", "run-dev.ps1"):
            source = (ROOT / relative).read_text(encoding="utf-8")
            self.assertIn("SGT_CREATION_RUNTIME_ROOT", source)
            self.assertNotIn("sgt-creation-runtime", source)

    def test_packaging_accepts_a_published_runtime(self) -> None:
        source = (ROOT / "scripts/build-creation-windows-pack.ps1").read_text(
            encoding="utf-8"
        )
        self.assertIn("SGT_CREATION_RUNTIME_BIN", source)
        self.assertIn("sgt_creation_runtime.manifest.json", source)


if __name__ == "__main__":
    unittest.main()
