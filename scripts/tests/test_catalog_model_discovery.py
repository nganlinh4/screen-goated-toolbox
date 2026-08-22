import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "catalog_model_discovery", ROOT / "scripts" / "catalog_model_discovery.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class CatalogModelDiscoveryTests(unittest.TestCase):
    def test_credential_pool_accepts_arrays_and_canonical_slots(self):
        dotenv = {
            "GEMINI_API_KEYS_JSON": '["array-a","array-b"]',
            "GEMINI_API_KEY": "primary",
            "GEMINI_API_KEY_2": "second",
            "GEMINI_API_KEY_02": "ignored",
            "GEMINI_API_KEY_0": "ignored",
        }
        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(
                MODULE.credential_pool("GEMINI_API_KEY", dotenv),
                ["array-a", "array-b", "primary", "second"],
            )

    def test_groq_free_rows_keep_exact_documented_ids_and_limits(self):
        document = """
        <table><tr><th>MODEL ID</th><th>RPM</th><th>RPD</th><th>TPM</th><th>TPD</th><th>ASH</th><th>ASD</th></tr>
        <tr><td>openai/gpt-oss-20b</td><td>30</td><td>1K</td><td>8K</td><td>200K</td><td>-</td><td>-</td></tr></table>
        """
        rows = MODULE.groq_free_rows(document)
        self.assertEqual(rows["openai/gpt-oss-20b"]["rpd"], "1K")
        self.assertNotIn("allam-2-7b", rows)

    def test_gemini_pricing_uses_documented_section_without_family_hardcoding(self):
        document = """
        <h2>Gemini 3.7 Flash</h2><table><tr><th></th><th>Free Tier</th></tr>
        <tr><td>Input price</td><td>Free of charge</td></tr></table>
        <h2>Gemini Paid Example</h2><table><tr><th></th><th>Free Tier</th></tr>
        <tr><td>Input price</td><td>Not available</td></tr></table>
        """
        sections = MODULE.gemini_pricing_sections(document)
        free = MODULE.gemini_pricing_status("Gemini 3.7 Flash", sections)
        paid = MODULE.gemini_pricing_status("Gemini Paid Example", sections)
        unknown = MODULE.gemini_pricing_status("Future Model", sections)
        self.assertEqual(free["status"], "documented-free")
        self.assertEqual(paid["status"], "documented-unavailable")
        self.assertEqual(unknown["status"], "not-documented")

    def test_report_flags_api_visibility_without_silently_qualifying_it(self):
        catalog = {
            "non_llm_ids": [],
            "model_profiles": {
                "groq:known": {"search_tool_enabled_by_default": False},
                "google:known-gemini": {"search_tool_enabled_by_default": False},
            },
            "models": [
                {"id": "groq-known-text", "provider": "groq", "full_name": "known", "enabled": True, "model_type": "Text"},
                {"id": "google-known-text", "provider": "google", "full_name": "known-gemini", "enabled": True, "model_type": "Text"},
            ],
        }
        gemini = {
            "models": [{"name": "models/api-only", "displayName": "API Only", "supportedGenerationMethods": ["generateContent"]}],
            "endpoint": "gemini", "documentation": "docs", "credential_count": 1,
            "successful_credentials": 1, "inventory_variants": 1, "inventory_fingerprints": [],
            "pagination_observed": False, "errors": [],
        }
        groq = {
            "models": [{"id": "allam-2-7b", "active": True}],
            "endpoint": "groq", "documentation": "docs", "credential_count": 1,
            "successful_credentials": 1, "inventory_variants": 1, "inventory_fingerprints": [], "errors": [],
        }
        report = MODULE.build_report(catalog, gemini, groq, None, "<table></table>")
        self.assertIn("api-only", report["gemini"]["listed_not_catalog"])
        self.assertEqual(report["groq"]["api_visible_not_documented_free"], ["allam-2-7b"])
        self.assertEqual(report["policy"]["catalog_mutation"], "never")


if __name__ == "__main__":
    unittest.main()
