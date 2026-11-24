"""Mobile release and device helpers resolve machine state outside the checkout."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
MOBILE_SCRIPTS = REPO / "mobile" / "scripts"
PWSH = shutil.which("pwsh")


def run_pwsh(script: str, env: dict[str, str]) -> subprocess.CompletedProcess:
    base = {key: value for key, value in os.environ.items()
            if key not in {"JAVA_HOME", "ANDROID_HOME", "ANDROID_SDK_ROOT", "SGT_SGTP_CONFIG"}}
    return subprocess.run(
        [PWSH, "-NoProfile", "-NonInteractive", "-Command", script],
        capture_output=True,
        encoding="utf-8",
        timeout=120,
        env={**base, **env},
    )


@unittest.skipUnless(PWSH, "PowerShell 7 is required")
class AndroidToolchainTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.mobile = self.root / "mobile"
        self.mobile.mkdir()
        self.sdk = self.root / "android sdk"
        self.sdk.mkdir()

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def resolve_sdk(self, env: dict[str, str]) -> subprocess.CompletedProcess:
        script = (
            f". '{MOBILE_SCRIPTS / 'android-toolchain.ps1'}'; "
            "Resolve-AndroidSdkRoot -MobileDir $env:TEST_MOBILE"
        )
        return run_pwsh(script, {"TEST_MOBILE": str(self.mobile), **env})

    def test_sdk_comes_from_escaped_local_properties(self):
        escaped = str(self.sdk).replace("\\", "\\\\").replace(":", "\\:")
        (self.mobile / "local.properties").write_text(f"sdk.dir={escaped}\n", encoding="utf-8")
        result = self.resolve_sdk({})
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertTrue(self.sdk.samefile(result.stdout.strip()))

    def test_environment_sdk_wins_and_missing_sdk_is_a_clear_error(self):
        result = self.resolve_sdk({"ANDROID_HOME": str(self.sdk)})
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertTrue(self.sdk.samefile(result.stdout.strip()))
        missing = self.resolve_sdk({})
        self.assertNotEqual(0, missing.returncode)
        self.assertIn("Android SDK not found", missing.stderr)

    def test_java_home_must_hold_a_jdk(self):
        script = f". '{MOBILE_SCRIPTS / 'android-toolchain.ps1'}'; Resolve-ReleaseJavaHome"
        self.assertIn("JAVA_HOME is not set", run_pwsh(script, {}).stderr)
        jdk = self.root / "jdk"
        (jdk / "bin").mkdir(parents=True)
        self.assertIn("no bin", run_pwsh(script, {"JAVA_HOME": str(jdk)}).stderr)
        (jdk / "bin" / "java.exe").write_bytes(b"")
        result = run_pwsh(script, {"JAVA_HOME": str(jdk)})
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertTrue(jdk.samefile(result.stdout.strip()))


@unittest.skipUnless(PWSH, "PowerShell 7 is required")
class SgtpConfigTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.repo = self.root / "repo"
        (self.repo / "mobile").mkdir(parents=True)
        self.local = self.root / "local"
        self.local.mkdir()

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def run_config(self, body: str) -> subprocess.CompletedProcess:
        script = f". '{MOBILE_SCRIPTS / 'sgtp-config.ps1'}'; {body}"
        return run_pwsh(script, {"LOCALAPPDATA": str(self.local), "TEST_REPO": str(self.repo)})

    def test_saved_endpoint_lives_outside_the_checkout(self):
        legacy = self.repo / "mobile" / ".sgtp.json"
        legacy.write_text(json.dumps({"host": "10.0.0.5", "port": 41000}), encoding="utf-8")
        result = self.run_config(
            "$c = Read-SgtpConfig $env:TEST_REPO; $c.port = 42000; Write-SgtpConfig $c; "
            "(Read-SgtpConfig $env:TEST_REPO).port"
        )
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("42000", result.stdout.strip())
        saved = json.loads((self.local / "SGT-Development" / "sgtp.json").read_text(encoding="utf-8-sig"))
        self.assertEqual("10.0.0.5", saved["host"])
        self.assertEqual(41000, json.loads(legacy.read_text(encoding="utf-8"))["port"],
                         "the checkout copy is never rewritten")

    def test_defaults_name_no_device_address(self):
        result = self.run_config("[string]::IsNullOrEmpty((Read-SgtpConfig $env:TEST_REPO).host)")
        self.assertEqual("True", result.stdout.strip(), result.stderr)


class ReleaseWrapperSourceTest(unittest.TestCase):
    def test_release_wrapper_has_no_machine_paths_and_gates_the_play_bundle(self):
        source = (REPO / "mobile" / "build-release.ps1").read_text(encoding="utf-8")
        self.assertNotIn("C:\\Users", source)
        self.assertIn(":androidApp:verifyPlayReleaseCompliance", source)
        self.assertNotIn(":androidApp:bundlePlayRelease", source)


if __name__ == "__main__":
    unittest.main()
