from __future__ import annotations

import copy
import importlib.util
import unittest
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


if __name__ == "__main__":
    unittest.main()
