import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class CreationRuntimeSourceBoundaryTests(unittest.TestCase):
    def test_build_and_development_use_the_external_runtime_checkout(self) -> None:
        for relative in ("scripts/build-creation-windows-pack.ps1", "run-dev.ps1"):
            source = (ROOT / relative).read_text(encoding="utf-8")
            self.assertIn("SGT_CREATION_RUNTIME_ROOT", source)
            self.assertIn("..\\sgt-creation-runtime", source)
            self.assertNotIn('"native\\sgt_3d_generator_runtime"', source)


if __name__ == "__main__":
    unittest.main()
