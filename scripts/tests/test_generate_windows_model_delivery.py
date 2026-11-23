import json
import sys
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))

from generate_windows_model_delivery import generate_delivery  # noqa: E402


class GenerateWindowsModelDeliveryTests(unittest.TestCase):
    def test_tracked_delivery_is_the_generated_empty_catalog(self):
        tracked = json.loads(
            (REPO / "model-delivery" / "windows-v1.json").read_text(encoding="utf-8")
        )
        self.assertEqual(tracked, generate_delivery({"schemaVersion": 1, "models": []}))
        self.assertEqual(tracked["models"], [])

    def test_packaged_models_pass_through_in_order(self):
        models = [{"id": "b-model"}, {"id": "a-model"}]
        delivery = generate_delivery({"schemaVersion": 1, "models": models})
        self.assertEqual([model["id"] for model in delivery["models"]], ["b-model", "a-model"])

    def test_rejects_unknown_schema_and_duplicate_ids(self):
        with self.assertRaises(ValueError):
            generate_delivery({"schemaVersion": 2, "models": []})
        with self.assertRaises(ValueError):
            generate_delivery({"schemaVersion": 1, "models": [{"id": "x"}, {"id": "x"}]})


if __name__ == "__main__":
    unittest.main()
