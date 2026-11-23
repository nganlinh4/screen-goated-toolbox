import json
import re
import subprocess
import tempfile
import tomllib
import unittest
import zipfile
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "pack-browser-ext.ps1"
EXTENSION = REPO / "src" / "overlay" / "computer_control" / "browser_ext"
SETUP = REPO / "src" / "overlay" / "computer_control" / "browser" / "setup.rs"


class PackBrowserExtensionTests(unittest.TestCase):
    def test_zip_matches_the_embedded_extension_with_the_cargo_version(self):
        embedded = set(
            re.findall(r'include_bytes!\("\.\./browser_ext/([^"/]+)"\)', SETUP.read_text("utf-8"))
        )
        cargo = tomllib.loads((REPO / "Cargo.toml").read_text("utf-8"))["package"]["version"]
        version = re.split(r"[-+]", cargo)[0]
        source_manifest = json.loads((EXTENSION / "manifest.json").read_text("utf-8"))

        with tempfile.TemporaryDirectory() as output:
            result = subprocess.run(
                [
                    "powershell", "-NoProfile", "-ExecutionPolicy", "Bypass",
                    "-File", str(SCRIPT), "-OutputDirectory", output,
                ],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("Development artifact only", result.stdout + result.stderr)
            archives = list(Path(output).iterdir())
            self.assertEqual([path.name for path in archives], [f"sgt-browser-bridge-{version}.zip"])
            with zipfile.ZipFile(archives[0]) as archive:
                self.assertEqual(set(archive.namelist()), embedded)
                manifest = json.loads(archive.read("manifest.json").decode("utf-8-sig"))
                for name in embedded - {"manifest.json"}:
                    self.assertEqual(archive.read(name), (EXTENSION / name).read_bytes(), name)

        self.assertNotIn("README.md", embedded)
        self.assertNotIn("bootstrap.js", embedded)
        self.assertEqual(manifest, {**source_manifest, "version": version})


if __name__ == "__main__":
    unittest.main()
