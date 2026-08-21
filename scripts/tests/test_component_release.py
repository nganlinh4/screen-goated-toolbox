import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "component_release.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("component_release", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def delivery(name: str, digest: str, size: int = 10) -> dict[str, object]:
    return {
        "asset": f"{name}-{digest[:16]}.bin",
        "downloadUrl": f"https://example.invalid/{name}.bin",
        "sizeBytes": size,
        "sha256": digest,
    }


def creation_contract(windows_digest: str = "1" * 64) -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "hostVersion": "5.4.3",
        "version": "0.9.0",
        "features": ["image_to_3d", "image_to_svg", "image_creator"],
        "windows": delivery("windows", windows_digest),
        "android": {
            "full": delivery("android-full-old", "2" * 64),
            "play": delivery("android-play-old", "3" * 64),
            "factoryClass": "old.Factory",
            "entries": [
                {
                    "archivePath": "runtime/old.jar",
                    "installPath": "runtime/old.jar",
                    "role": "factory_dex",
                    "sizeBytes": 5,
                    "sha256": "4" * 64,
                }
            ],
        },
    }


def creation_package(
    full: dict[str, object] | None = None,
    play: dict[str, object] | None = None,
) -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "hostVersion": "5.4.3",
        "version": "0.9.0",
        "features": ["image_to_3d", "image_to_svg", "image_creator"],
        "windows": delivery("stale-windows", "5" * 64),
        "android": {
            "full": full or delivery("android-full-new", "6" * 64),
            "play": play or delivery("android-play-new", "7" * 64),
            "factoryClass": "new.Factory",
            "entries": [
                {
                    "archivePath": "runtime/new.jar",
                    "installPath": "runtime/new.jar",
                    "role": "factory_dex",
                    "sizeBytes": 6,
                    "sha256": "8" * 64,
                }
            ],
        },
    }


class ComponentReleaseTests(unittest.TestCase):
    def test_upload_uses_the_declared_asset_name_when_local_name_differs(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory, "local-build-name.bin")
            source.write_bytes(b"candidate")
            asset = {
                "asset": "immutable-declared-name.bin",
                "path": str(source),
                "sizeBytes": len(b"candidate"),
                "sha256": hashlib.sha256(b"candidate").hexdigest(),
            }
            uploaded = []

            def capture_upload(*arguments):
                path = Path(arguments[3])
                uploaded.append((path.name, path.read_bytes()))

            with (
                mock.patch.object(MODULE, "release_assets", return_value={}),
                mock.patch.object(MODULE, "gh", side_effect=capture_upload),
                mock.patch.object(MODULE, "verify_remote"),
            ):
                MODULE.upload_missing(MODULE.REPOSITORY, MODULE.STAGING_TAG, asset)

            self.assertEqual(uploaded, [(asset["asset"], b"candidate")])

    def test_selected_component_merges_without_changing_sibling(self):
        base = {
            "components": [
                {"id": "one", "asset": "old-one.zip", "sizeBytes": 1},
                {"id": "two", "asset": "old-two.zip", "sizeBytes": 2},
            ]
        }
        package = {
            "components": [
                {
                    "id": "one",
                    "asset": "one-v2-0123456789abcdef.zip",
                    "assetPath": "ignored.zip",
                    "sizeBytes": 3,
                },
                {"id": "two", "asset": "new-two.zip", "sizeBytes": 4},
            ]
        }
        candidate = MODULE.merge_candidate(base, package, {"one"})
        self.assertEqual(candidate["components"][0]["sizeBytes"], 3)
        self.assertNotIn("assetPath", candidate["components"][0])
        self.assertEqual(candidate["components"][1]["asset"], "old-two.zip")

    def test_url_rewrite_is_limited_to_exact_selected_assets(self):
        candidate = {
            "components": [
                {"asset": "selected.zip", "downloadUrl": "production"},
                {"asset": "stable.zip", "downloadUrl": "production-stable"},
            ],
            "archive": {"url": "https://example/selected-model.zip", "sizeBytes": 1},
        }
        MODULE.rewrite_urls(
            candidate,
            {"selected.zip", "selected-model.zip"},
            MODULE.STAGING_TAG,
            MODULE.REPOSITORY,
        )
        self.assertIn("sgt-runtime-staging/selected.zip", candidate["components"][0]["downloadUrl"])
        self.assertEqual(candidate["components"][1]["downloadUrl"], "production-stable")
        self.assertIn("sgt-runtime-staging/selected-model.zip", candidate["archive"]["url"])

    def test_local_asset_must_match_exact_manifest_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            payload = b"candidate"
            digest = hashlib.sha256(payload).hexdigest()
            asset = root / f"worker-{digest[:16]}.zip"
            asset.write_bytes(payload)
            package = {
                "component": {
                    "id": "worker",
                    "asset": asset.name,
                    "sizeBytes": len(payload),
                    "sha256": digest,
                }
            }
            assets = MODULE.collect_local_assets(root, package, root, set())
            self.assertEqual(assets[0]["asset"], asset.name)
            package["component"]["sizeBytes"] += 1
            with self.assertRaises(ValueError):
                MODULE.collect_local_assets(root, package, root, set())

    def test_staging_index_contains_no_machine_asset_paths(self):
        value = {
            "id": "worker",
            "asset": "worker.zip",
            "assetPath": r"C:\temp\worker.zip",
            "files": [{"path": "bin/worker.exe"}],
        }
        cleaned = MODULE.clean_copy(value)
        self.assertNotIn("assetPath", json.dumps(cleaned))

    def test_android_pair_replaces_only_android_records_and_shared_metadata(self):
        base = creation_contract()
        original = copy.deepcopy(base)
        package = creation_package()
        package["android"]["full"]["assetPath"] = "ignored.zip"

        candidate = MODULE.merge_candidate(
            base,
            package,
            {"android/full", "android/play"},
        )

        self.assertEqual(candidate["windows"], original["windows"])
        self.assertEqual(candidate["version"], original["version"])
        self.assertEqual(candidate["android"]["full"]["sha256"], "6" * 64)
        self.assertEqual(candidate["android"]["play"]["sha256"], "7" * 64)
        self.assertEqual(candidate["android"]["factoryClass"], "new.Factory")
        self.assertEqual(candidate["android"]["entries"][0]["sha256"], "8" * 64)
        self.assertNotIn("assetPath", candidate["android"]["full"])
        self.assertEqual(base, original)

    def test_windows_and_android_pair_replace_one_atomic_contract(self):
        base = creation_contract()
        package = creation_package()

        candidate = MODULE.merge_candidate(
            base,
            package,
            {"windows", "android/full", "android/play"},
        )

        self.assertEqual(candidate["windows"]["sha256"], "5" * 64)
        self.assertEqual(candidate["android"]["full"]["sha256"], "6" * 64)
        self.assertEqual(candidate["android"]["play"]["sha256"], "7" * 64)
        self.assertEqual(candidate["android"]["factoryClass"], "new.Factory")

    def test_android_pair_rejects_partial_or_unknown_selection(self):
        invalid = [
            {"android/full"},
            {"android/play"},
            {"android"},
            {"android/full", "android/play", "android/unknown"},
        ]
        for selected in invalid:
            with self.subTest(selected=selected):
                with self.assertRaisesRegex(ValueError, "selected together"):
                    MODULE.merge_candidate(
                        creation_contract(), creation_package(), selected
                    )

    def test_android_pair_rejects_missing_required_package_metadata(self):
        mutations = (
            lambda android: android.pop("play"),
            lambda android: android.pop("factoryClass"),
            lambda android: android.__setitem__("entries", []),
            lambda android: android["entries"][0].__setitem__("sizeBytes", 0),
        )
        for mutate in mutations:
            package = creation_package()
            mutate(package["android"])
            with self.assertRaises(ValueError):
                MODULE.merge_candidate(
                    creation_contract(),
                    package,
                    {"android/full", "android/play"},
                )

    def test_android_assets_cannot_alias_preserved_windows_asset(self):
        base = creation_contract()
        package = creation_package(full=copy.deepcopy(base["windows"]))
        names = {
            package["android"]["full"]["asset"],
            package["android"]["play"]["asset"],
        }
        with self.assertRaisesRegex(ValueError, "tracked Windows"):
            MODULE.validate_preserved_assets(
                base,
                names,
                {"android/full", "android/play"},
            )

    def test_stage_uses_tracked_contract_instead_of_previous_staging_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            records = {}
            for owner, payload in (
                ("full", b"full candidate"),
                ("play", b"play candidate"),
            ):
                digest = hashlib.sha256(payload).hexdigest()
                path = root / f"android-{owner}-{digest[:16]}.bin"
                path.write_bytes(payload)
                records[owner] = {
                    **delivery(f"android-{owner}", digest, len(payload)),
                    "asset": path.name,
                    "assetPath": str(path),
                }
            tracked = root / "tracked.json"
            package_path = root / "package.json"
            tracked_value = creation_contract("a" * 64)
            package_value = creation_package(records["full"], records["play"])
            tracked.write_text(json.dumps(tracked_value), encoding="utf-8")
            package_path.write_text(json.dumps(package_value), encoding="utf-8")
            old_manifest = creation_contract("b" * 64)
            index = MODULE.empty_index(MODULE.REPOSITORY)
            index["contracts"]["component-delivery/runtime.json"] = {
                "manifest": old_manifest,
                "assets": [{"owner": "obsolete", "asset": "obsolete.bin"}],
            }
            args = SimpleNamespace(
                repository=MODULE.REPOSITORY,
                package_manifest=str(package_path),
                tracked_manifest=str(tracked),
                contract_relative="component-delivery/runtime.json",
                select=["android/full", "android/play"],
                repo_root=str(root),
                asset_root=str(root),
                cache_root=str(root / "cache"),
            )
            with (
                mock.patch.object(MODULE, "ensure_staging_release"),
                mock.patch.object(MODULE, "load_staging_index", return_value=index),
                mock.patch.object(MODULE, "upload_missing"),
                mock.patch.object(MODULE, "git_commit", return_value="commit"),
                mock.patch.object(MODULE, "gh"),
                mock.patch.object(MODULE, "verify_index"),
                mock.patch.object(MODULE, "remove_unreferenced_staging_assets"),
            ):
                MODULE.stage(args)

            staged = index["contracts"]["component-delivery/runtime.json"]
            self.assertEqual(staged["manifest"]["windows"], tracked_value["windows"])
            self.assertEqual(staged["selectors"], ["android/full", "android/play"])
            self.assertEqual(
                {asset["owner"] for asset in staged["assets"]},
                {"android/full", "android/play"},
            )

    def test_stage_does_not_publish_contract_before_public_bytes_propagate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            payload = b"candidate"
            digest = hashlib.sha256(payload).hexdigest()
            name = f"worker-{digest[:16]}.bin"
            asset = root / name
            asset.write_bytes(payload)
            tracked = root / "tracked.json"
            package = root / "package.json"
            tracked.write_text(
                json.dumps(
                    {
                        "component": {
                            "id": "worker",
                            "asset": "old-0000000000000000.bin",
                            "downloadUrl": "https://example.invalid/old.bin",
                            "sizeBytes": 1,
                            "sha256": "0" * 64,
                        }
                    }
                ),
                encoding="utf-8",
            )
            package.write_text(
                json.dumps(
                    {
                        "component": {
                            "id": "worker",
                            "asset": name,
                            "assetPath": str(asset),
                            "sizeBytes": len(payload),
                            "sha256": digest,
                        }
                    }
                ),
                encoding="utf-8",
            )
            args = SimpleNamespace(
                repository=MODULE.REPOSITORY,
                package_manifest=str(package),
                tracked_manifest=str(tracked),
                contract_relative="component-delivery/worker.json",
                select=["worker"],
                repo_root=str(root),
                asset_root=str(root),
                cache_root=str(root / "cache"),
            )
            contract = (
                root
                / "cache"
                / "staging"
                / "contracts"
                / "component-delivery"
                / "worker.json"
            )
            with (
                mock.patch.object(MODULE, "ensure_staging_release"),
                mock.patch.object(
                    MODULE,
                    "load_staging_index",
                    return_value=MODULE.empty_index(MODULE.REPOSITORY),
                ),
                mock.patch.object(
                    MODULE,
                    "upload_missing",
                    side_effect=RuntimeError("public URL is still 404"),
                ),
                mock.patch.object(MODULE, "gh") as gh,
            ):
                with self.assertRaisesRegex(RuntimeError, "still 404"):
                    MODULE.stage(args)

            self.assertFalse(contract.exists())
            gh.assert_not_called()

    def test_verify_staging_uses_staging_transport_verification(self):
        digest = "1" * 64
        name = f"worker-{digest[:16]}.bin"
        index = MODULE.empty_index(MODULE.REPOSITORY)
        index["contracts"]["component-delivery/worker.json"] = {
            "assets": [
                {
                    "owner": "worker",
                    "asset": name,
                    "sizeBytes": 10,
                    "sha256": digest,
                }
            ]
        }
        args = SimpleNamespace(repository=MODULE.REPOSITORY)
        def transport(repository, tag, asset, size, sha256):
            self.assertEqual(repository, MODULE.REPOSITORY)
            self.assertEqual(tag, MODULE.STAGING_TAG)
            self.assertEqual(asset, name)
            self.assertEqual((size, sha256), (10, digest))

        with (
            mock.patch.object(MODULE, "load_staging_index", return_value=index),
            mock.patch.object(
                MODULE,
                "release_assets",
                return_value={MODULE.INDEX_ASSET: {}, name: {}},
            ),
            mock.patch.object(MODULE, "verify_remote", side_effect=transport) as remote,
        ):
            MODULE.verify(args)

        remote.assert_called_once()

    def test_verify_staging_rejects_unindexed_release_assets(self):
        args = SimpleNamespace(repository=MODULE.REPOSITORY)
        with (
            mock.patch.object(
                MODULE,
                "load_staging_index",
                return_value=MODULE.empty_index(MODULE.REPOSITORY),
            ),
            mock.patch.object(
                MODULE,
                "release_assets",
                return_value={MODULE.INDEX_ASSET: {}, "orphan.bin": {}},
            ),
        ):
            with self.assertRaisesRegex(RuntimeError, "unindexed assets: orphan.bin"):
                MODULE.verify(args)

    def test_contract_relative_rejects_absolute_traversal_and_drive_paths(self):
        for value in ["/component.json", "../component.json", "C:/component.json"]:
            with self.subTest(value=value):
                with self.assertRaisesRegex(ValueError, "safe JSON path"):
                    MODULE.safe_contract_relative(value)
        self.assertEqual(
            MODULE.safe_contract_relative(r"component-delivery\worker.json"),
            "component-delivery/worker.json",
        )

    def test_discard_removes_stale_local_contract_without_remote_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            relative = "component-delivery/worker.json"
            contract = root / "staging" / "contracts" / relative
            sibling = contract.with_name("other.json")
            contract.parent.mkdir(parents=True)
            contract.write_text("{}", encoding="utf-8")
            sibling.write_text("{}", encoding="utf-8")
            args = SimpleNamespace(
                repository=MODULE.REPOSITORY,
                contract_relative=relative,
                cache_root=str(root),
            )
            with (
                mock.patch.object(
                    MODULE,
                    "load_staging_index",
                    return_value=MODULE.empty_index(MODULE.REPOSITORY),
                ),
                mock.patch.object(MODULE, "gh") as gh,
            ):
                self.assertEqual(MODULE.discard(args), 0)

            self.assertFalse(contract.exists())
            self.assertTrue(sibling.exists())
            gh.assert_not_called()

    def test_stage_rejects_partial_android_selection_before_remote_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tracked = root / "tracked.json"
            package = root / "package.json"
            tracked.write_text(json.dumps(creation_contract()), encoding="utf-8")
            package.write_text(json.dumps(creation_package()), encoding="utf-8")
            args = SimpleNamespace(
                repository=MODULE.REPOSITORY,
                package_manifest=str(package),
                tracked_manifest=str(tracked),
                contract_relative="component-delivery/runtime.json",
                select=["android/full"],
                repo_root=str(root),
                asset_root=str(root),
                cache_root=str(root / "cache"),
            )
            with mock.patch.object(MODULE, "ensure_staging_release") as remote:
                with self.assertRaisesRegex(ValueError, "selected together"):
                    MODULE.stage(args)
            remote.assert_not_called()

    def test_promote_preserves_current_tracked_windows_record(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            payloads = {}
            assets = []
            package_records = {}
            for owner, (name, payload) in zip(
                ("android/full", "android/play"),
                (("android-full", b"full"), ("android-play", b"play")),
                strict=True,
            ):
                digest = hashlib.sha256(payload).hexdigest()
                content_name = f"{name}-{digest[:16]}.bin"
                package_records[owner.rsplit("/", 1)[-1]] = delivery(
                    name, digest, len(payload)
                )
                package_records[owner.rsplit("/", 1)[-1]]["asset"] = content_name
                assets.append(
                    {
                        "owner": owner,
                        "asset": content_name,
                        "sizeBytes": len(payload),
                        "sha256": digest,
                    }
                )
                payloads[content_name] = payload
            staged_manifest = MODULE.merge_candidate(
                creation_contract("a" * 64),
                creation_package(package_records["full"], package_records["play"]),
                {"android/full", "android/play"},
            )
            current = creation_contract("c" * 64)
            tracked = root / "tracked.json"
            output = root / "promoted.json"
            tracked.write_text(json.dumps(current), encoding="utf-8")
            index = MODULE.empty_index(MODULE.REPOSITORY)
            index["contracts"]["component-delivery/runtime.json"] = {
                "manifest": staged_manifest,
                "assets": assets,
                "selectors": ["android/full", "android/play"],
            }
            args = SimpleNamespace(
                repository=MODULE.REPOSITORY,
                contract_relative="component-delivery/runtime.json",
                output=str(output),
                apply_tracked=str(tracked),
                clean_staging=False,
            )

            def download(_repository, _tag, name, path):
                path.write_bytes(payloads[name])

            with (
                mock.patch.object(MODULE, "load_staging_index", return_value=index),
                mock.patch.object(MODULE, "download_asset", side_effect=download),
                mock.patch.object(MODULE, "upload_missing") as upload,
            ):
                MODULE.promote(args)

            promoted = json.loads(tracked.read_text(encoding="utf-8"))
            self.assertEqual(promoted["windows"], current["windows"])
            self.assertEqual(promoted["android"]["full"]["sha256"], assets[0]["sha256"])
            self.assertEqual(promoted["android"]["play"]["sha256"], assets[1]["sha256"])
            self.assertIn(MODULE.PRODUCTION_TAG, promoted["android"]["full"]["downloadUrl"])
            self.assertEqual(upload.call_count, 2)

    def test_promote_rejects_partial_android_selection_before_transfer(self):
        index = MODULE.empty_index(MODULE.REPOSITORY)
        digest = "1" * 64
        index["contracts"]["component-delivery/runtime.json"] = {
            "manifest": creation_contract(),
            "assets": [
                {
                    "owner": "android/full",
                    "asset": f"full-{digest[:16]}.bin",
                    "sizeBytes": 1,
                    "sha256": digest,
                }
            ],
            "selectors": ["android/full"],
        }
        args = SimpleNamespace(
            repository=MODULE.REPOSITORY,
            contract_relative="component-delivery/runtime.json",
            output="unused.json",
            apply_tracked=None,
            clean_staging=False,
        )
        with (
            mock.patch.object(MODULE, "load_staging_index", return_value=index),
            mock.patch.object(MODULE, "download_asset") as download,
            mock.patch.object(MODULE, "upload_missing") as upload,
        ):
            with self.assertRaisesRegex(ValueError, "selected together"):
                MODULE.promote(args)
        download.assert_not_called()
        upload.assert_not_called()


if __name__ == "__main__":
    unittest.main()
