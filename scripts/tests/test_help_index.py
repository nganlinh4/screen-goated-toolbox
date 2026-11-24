import base64
import gzip
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path, PurePosixPath

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from help_index_build import write_package
from help_index_corpus import MAX_CHUNK_CHARS, chunks, excluded, platforms_for
from help_index_embeddings import DIMENSIONS, pack_vector


class HelpSourceIndexTest(unittest.TestCase):
    def test_corpus_keeps_product_code_and_parity_docs(self):
        for path in ("src/lib.rs", "mobile/androidApp/Example.kt", ".claude/parity/help.md"):
            self.assertFalse(excluded(PurePosixPath(path)))
        for path in (".env", "docs/RELEASING.local.md", "third_party/lib.rs", "workspace/build/log.txt"):
            self.assertTrue(excluded(PurePosixPath(path)))
        self.assertEqual(platforms_for(PurePosixPath("src/lib.rs")), ["windows"])
        self.assertEqual(platforms_for(PurePosixPath("mobile/androidApp/Example.kt")), ["android"])

    def test_long_source_lines_keep_all_text(self):
        text = "a" * MAX_CHUNK_CHARS + "b" * MAX_CHUNK_CHARS + "c"
        self.assertEqual("".join(piece for _, piece in chunks(text)), text)

    def test_package_is_content_addressed_and_retains_signed_vectors(self):
        vector = pack_vector([1.0, -1.0] + [0.0] * (DIMENSIONS - 2))
        self.assertEqual(vector[:2], bytes([127, 129]))
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            entries = [{
                "path": "src/lib.rs",
                "text": "source",
                "startLine": 1,
                "platforms": ["windows"],
                "vector": base64.b64encode(vector).decode(),
            }]
            manifest = write_package(output, "a" * 40, "gemini-embedding-2", entries)
            contract = json.loads(manifest.read_text())["helpIndex"]
            asset = output / contract["asset"]
            compressed = asset.read_bytes()
            self.assertEqual(hashlib.sha256(compressed).hexdigest(), contract["sha256"])
            expanded = gzip.decompress(compressed)
            self.assertEqual(hashlib.sha256(expanded).hexdigest(), contract["expandedSha256"])
            records = [json.loads(line) for line in expanded.splitlines()]
            self.assertEqual(records[0]["dimensions"], DIMENSIONS)
            self.assertEqual(records[1:], entries)
            write_package(output, "a" * 40, "gemini-embedding-2", entries)
            self.assertEqual(list(output.glob("*.gz")), [asset])
            self.assertEqual(asset.read_bytes(), compressed)


if __name__ == "__main__":
    unittest.main()
