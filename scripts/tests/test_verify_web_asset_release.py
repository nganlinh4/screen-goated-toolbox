import contextlib
import importlib.util
import io
from pathlib import Path
import sys
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "verify_web_asset_release.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("verify_web_asset_release", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ManifestArgumentsTest(unittest.TestCase):
    def test_manifests_have_no_checkout_default(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            MODULE.build_parser().parse_args([])

    def test_explicit_manifests_are_accepted(self):
        args = MODULE.build_parser().parse_args(["--packages", "p.json", "--output", "d.json"])
        self.assertEqual(("p.json", "d.json"), (args.packages, args.output))


if __name__ == "__main__":
    unittest.main()
