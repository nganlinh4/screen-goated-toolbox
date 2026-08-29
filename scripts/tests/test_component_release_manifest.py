import importlib.util
from pathlib import Path
import sys
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "component_release.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("component_release_manifest_tests", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class ComponentReleaseManifestTests(unittest.TestCase):
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

    def test_full_component_manifest_replaces_legacy_component_ids(self):
        base = {
            "schemaVersion": 1,
            "components": [
                {"id": "legacy-one", "asset": "one.zip"},
                {"id": "legacy-two", "asset": "two.zip"},
            ],
        }
        package = {
            "schemaVersion": 1,
            "components": [
                {
                    "id": "combined",
                    "asset": "combined.zip",
                    "assetPath": "ignored.zip",
                }
            ],
        }
        candidate = MODULE.merge_candidate(base, package, set())
        self.assertEqual([item["id"] for item in candidate["components"]], ["combined"])
        self.assertNotIn("assetPath", candidate["components"][0])

    def test_selected_component_must_exist_in_tracked_contract(self):
        base = {"components": [{"id": "existing", "asset": "old.zip"}]}
        package = {"components": [{"id": "replacement", "asset": "new.zip"}]}
        with self.assertRaisesRegex(ValueError, "absent from the tracked contract"):
            MODULE.merge_candidate(base, package, {"replacement"})
