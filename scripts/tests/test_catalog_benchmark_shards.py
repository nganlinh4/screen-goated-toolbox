import importlib.util
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "catalog_benchmark_shards", ROOT / "scripts" / "catalog_benchmark_shards.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class CatalogBenchmarkShardTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import json

        cls.catalog = json.loads((ROOT / "catalog" / "model_catalog.json").read_text("utf-8"))

    def test_every_hosted_general_model_is_covered_once(self):
        shards = MODULE.build_shards(
            self.catalog, set(), {"text", "coordinate", "ocr"}
        )
        covered = [
            model_id
            for shard in shards
            for model_id in shard["catalog_models"].split(",")
        ]
        expected = {
            model["id"]
            for model in self.catalog["models"]
            if model.get("enabled")
            and model["provider"] in MODULE.HOSTED_PROVIDERS
            and model["model_type"] in {"Text", "Vision"}
            and model["id"] not in set(self.catalog["non_llm_ids"])
            and not self.catalog["model_profiles"][
                f"{model['provider']}:{model['full_name']}"
            ]["search_tool_enabled_by_default"]
        }
        self.assertEqual(set(covered), expected)
        self.assertEqual(len(covered), len(set(covered)))

    def test_openrouter_and_nvidia_use_provider_wide_shards(self):
        shards = MODULE.build_shards(
            self.catalog, set(), {"text", "coordinate", "ocr"}
        )
        for provider in ("openrouter", "nvidia"):
            provider_shards = [shard for shard in shards if shard["provider"] == provider]
            self.assertEqual(len(provider_shards), 1)
        nvidia = next(shard for shard in shards if shard["provider"] == "nvidia")
        self.assertEqual(nvidia["models"], "")


if __name__ == "__main__":
    unittest.main()
