from __future__ import annotations

import copy
import io
import importlib.util
import unittest
import zipfile
from pathlib import Path


VERIFIER_PATH = Path(__file__).resolve().parents[1] / "verify_creation_runtime_release.py"
SPEC = importlib.util.spec_from_file_location(
    "verify_creation_runtime_release",
    VERIFIER_PATH,
)
assert SPEC is not None and SPEC.loader is not None
verifier = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verifier)


DIGESTS = {
    "windows": "1" * 64,
    "android.full": "2" * 64,
    "android.play": "3" * 64,
}


def delivery(label: str) -> dict[str, object]:
    digest = DIGESTS[label]
    asset = verifier.expected_asset_name(label, digest)
    return {
        "asset": asset,
        "downloadUrl": verifier.RUNTIME_BUNDLES + asset,
        "sizeBytes": 1024,
        "sha256": digest,
    }


def valid_manifest() -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "version": "1.2.3",
        "features": ["image_to_3d", "image_to_svg", "image_creator"],
        "windows": delivery("windows"),
        "android": {
            "full": delivery("android.full"),
            "play": delivery("android.play"),
            "entries": [
                {
                    "archivePath": "runtime/runtime.dex.jar",
                    "installPath": "runtime/runtime.dex.jar",
                    "role": "factory_dex",
                    "sizeBytes": 4,
                    "sha256": "054edec1d0211f624fed0cbca9d4f9400b0e491c43742af2c5b0abebf0c990d8",
                },
                {
                    "archivePath": "lib/arm64-v8a/runtime.so",
                    "installPath": "lib/arm64-v8a/runtime.so",
                    "role": "native_library",
                    "sizeBytes": 4,
                    "sha256": "9f64a747e1b97f131fabb6b447296c9b6f0201e79fb3c5356e6c77e89b6a806a",
                },
            ],
        },
    }


def record(manifest: dict[str, object], label: str) -> dict[str, object]:
    if label == "windows":
        return manifest["windows"]  # type: ignore[return-value]
    android = manifest["android"]  # type: ignore[assignment]
    key = "full" if label == "android.full" else "play"
    return android[key]  # type: ignore[index,return-value]


class CreationRuntimeReleaseVerifierTests(unittest.TestCase):
    def test_accepts_exact_feature_set_and_all_delivery_records(self) -> None:
        manifest = valid_manifest()
        manifest["features"] = ["image_creator", "image_to_3d", "image_to_svg"]

        records = verifier.validate_manifest(manifest)

        self.assertEqual(
            [entry[0] for entry in records],
            ["windows", "android.full", "android.play"],
        )

    def test_rejects_missing_extra_and_duplicate_features(self) -> None:
        invalid_sets = {
            "missing": ["image_to_3d", "image_to_svg"],
            "extra": [
                "image_to_3d",
                "image_to_svg",
                "image_creator",
                "future_tool",
            ],
            "duplicate": [
                "image_to_3d",
                "image_to_svg",
                "image_creator",
                "image_creator",
            ],
        }
        for name, features in invalid_sets.items():
            with self.subTest(name=name):
                manifest = valid_manifest()
                manifest["features"] = features
                with self.assertRaisesRegex(RuntimeError, "feature"):
                    verifier.validate_manifest(manifest)

    def test_rejects_non_content_addressed_asset_name_for_every_target(self) -> None:
        invalid_names = {
            "windows": "sgt-creation-runtime-windows-x64.exe",
            "android.full": "sgt-creation-runtime-android-arm64.zip",
            "android.play": "sgt-creation-runtime-android.aar",
        }
        for label, invalid_name in invalid_names.items():
            with self.subTest(label=label):
                manifest = valid_manifest()
                target = record(manifest, label)
                target["asset"] = invalid_name
                target["downloadUrl"] = verifier.RUNTIME_BUNDLES + invalid_name
                with self.assertRaisesRegex(RuntimeError, "content-addressed"):
                    verifier.validate_manifest(manifest)

    def test_rejects_wrong_digest_prefix_for_every_target(self) -> None:
        for label in DIGESTS:
            with self.subTest(label=label):
                manifest = valid_manifest()
                target = record(manifest, label)
                asset = str(target["asset"])
                target["asset"] = asset.replace(DIGESTS[label][:16], "f" * 16)
                target["downloadUrl"] = verifier.RUNTIME_BUNDLES + str(target["asset"])
                with self.assertRaisesRegex(RuntimeError, "content-addressed"):
                    verifier.validate_manifest(manifest)

    def test_rejects_non_immutable_url_for_every_target(self) -> None:
        for label in DIGESTS:
            with self.subTest(label=label):
                manifest = valid_manifest()
                target = record(manifest, label)
                target["downloadUrl"] = "https://example.invalid/" + str(target["asset"])
                with self.assertRaisesRegex(RuntimeError, "URL is not immutable"):
                    verifier.validate_manifest(manifest)

    def test_rejects_invalid_size_and_sha_for_every_target(self) -> None:
        for label in DIGESTS:
            with self.subTest(label=label, field="size"):
                manifest = valid_manifest()
                record(manifest, label)["sizeBytes"] = 0
                with self.assertRaisesRegex(RuntimeError, "identity is invalid"):
                    verifier.validate_manifest(manifest)
            with self.subTest(label=label, field="sha256"):
                manifest = valid_manifest()
                record(manifest, label)["sha256"] = "A" * 64
                with self.assertRaisesRegex(RuntimeError, "identity is invalid"):
                    verifier.validate_manifest(manifest)

    def test_validation_does_not_mutate_manifest(self) -> None:
        manifest = valid_manifest()
        original = copy.deepcopy(manifest)

        verifier.validate_manifest(manifest)

        self.assertEqual(manifest, original)

    def test_android_archives_must_match_the_shared_member_identities(self) -> None:
        entries = verifier.android_entries(valid_manifest()["android"])
        full = io.BytesIO()
        with zipfile.ZipFile(full, "w") as archive:
            archive.writestr("runtime/runtime.dex.jar", bytes((0, 1, 2, 3)))
            archive.writestr("lib/arm64-v8a/runtime.so", bytes((1, 2, 3, 4)))
        play = io.BytesIO()
        with zipfile.ZipFile(play, "w") as archive:
            archive.writestr("jni/arm64-v8a/runtime.so", bytes((1, 2, 3, 4)))

        verifier.verify_android_archive("android.full", full.getvalue(), entries)
        verifier.verify_android_archive("android.play", play.getvalue(), entries)

        stale = io.BytesIO()
        with zipfile.ZipFile(stale, "w") as archive:
            archive.writestr("jni/arm64-v8a/runtime.so", b"stale")
        with self.assertRaisesRegex(RuntimeError, "member size changed"):
            verifier.verify_android_archive("android.play", stale.getvalue(), entries)


if __name__ == "__main__":
    unittest.main()
