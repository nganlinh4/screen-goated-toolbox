import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "package_qwen3_runtime.py"
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("package_qwen3_runtime", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)
REPO = SCRIPT.parents[1]
TRACKED = REPO / "component-delivery/windows/qwen-runtime-v1.json"


class VerifiedDeliveryReuseTests(unittest.TestCase):
    def setUp(self):
        self.delivery = json.loads(TRACKED.read_text(encoding="utf-8"))
        component = self.delivery["windows"]["components"][0]
        self.runtime = copy.deepcopy(component["assets"][0])
        self.runtime.pop("downloadUrl")
        self.runtime["assetPath"] = "generated-runtime.zip"
        self.runtime_files = [
            copy.deepcopy(item)
            for item in component["files"]
            if item["archiveIndex"] == 0
        ]

    def validate(self, delivery):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            (output / "sgt_qwen3_runtime.delivery.json").write_text(
                json.dumps(delivery), encoding="utf-8"
            )
            return MODULE.require_delivery_without_libtorch_rebuild(
                output, MODULE.DEFAULT_VERSION, self.runtime, self.runtime_files
            )

    def test_accepts_reviewed_delivery_without_large_source_archive(self):
        self.assertEqual(
            "sgt_qwen3_runtime.delivery.json", self.validate(self.delivery).name
        )

    def test_rejects_runtime_pack_that_differs_from_local_bytes(self):
        changed = copy.deepcopy(self.delivery)
        changed["windows"]["components"][0]["assets"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(RuntimeError, "runtime pack"):
            self.validate(changed)

    def test_rejects_changed_libtorch_selection_policy(self):
        changed = copy.deepcopy(self.delivery)
        component = changed["windows"]["components"][0]
        component["files"] = [
            item for item in component["files"] if item["path"] != "bin/x64/uv.dll"
        ]
        component["unpackedSizeBytes"] = sum(
            item["sizeBytes"] for item in component["files"]
        )
        with self.assertRaisesRegex(RuntimeError, "selection policy"):
            self.validate(changed)

    def test_rejects_duplicate_libtorch_inventory_entry(self):
        changed = copy.deepcopy(self.delivery)
        component = changed["windows"]["components"][0]
        duplicate = copy.deepcopy(
            next(item for item in component["files"] if item["archiveIndex"] == 1)
        )
        component["files"].append(duplicate)
        component["unpackedSizeBytes"] += duplicate["sizeBytes"]
        with self.assertRaisesRegex(RuntimeError, "selection policy"):
            self.validate(changed)


if __name__ == "__main__":
    unittest.main()
