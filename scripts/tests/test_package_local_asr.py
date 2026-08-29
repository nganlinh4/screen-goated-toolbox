import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "package_local_asr.py"
SPEC = importlib.util.spec_from_file_location("package_local_asr", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class NormalizedTextTests(unittest.TestCase):
    def test_crlf_checkout_matches_reviewed_lf_identity(self):
        expected = b"line one\nline two\n"
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "LICENSE"
            path.write_bytes(expected.replace(b"\n", b"\r\n"))

            actual = MODULE.normalized_text_bytes(
                path, len(expected), hashlib.sha256(expected).hexdigest()
            )

        self.assertEqual(expected, actual)

    def test_unreviewed_content_is_rejected(self):
        expected = b"reviewed\n"
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "LICENSE"
            path.write_bytes(b"different\r\n")

            with self.assertRaisesRegex(ValueError, "identity mismatch"):
                MODULE.normalized_text_bytes(
                    path, len(expected), hashlib.sha256(expected).hexdigest()
                )


if __name__ == "__main__":
    unittest.main()
