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


class RuntimePackageTests(unittest.TestCase):
    def test_package_is_content_addressed_and_records_every_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            (source / "a.dll").write_bytes(b"runtime")
            (source / "LICENSE").write_bytes(b"license\n")
            output = root / "out"
            output.mkdir()
            files = [
                ("bin/x64/a.dll", source / "a.dll"),
                ("licenses/LICENSE.txt", source / "LICENSE"),
            ]

            first = MODULE.package_component(output, "onnx-directml-runtime", "1.0", files)
            second = MODULE.package_component(output, "onnx-directml-runtime", "1.0", files)

            self.assertEqual(first, second)
            archive = output / first["asset"]
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            self.assertEqual(first["sha256"], digest)
            self.assertEqual(first["asset"], f"onnx-directml-runtime-1.0-{digest[:16]}.zip")
            self.assertEqual(
                [record["path"] for record in first["files"]],
                ["bin/x64/a.dll", "licenses/LICENSE.txt"],
            )
            self.assertEqual(first["unpackedSizeBytes"], len(b"runtime") + len(b"license\n"))


if __name__ == "__main__":
    unittest.main()
