import hashlib
import io
import unittest
from unittest import mock

from scripts import verify_update_catalog_sources as verifier
from scripts.update_catalog_payload import sequence_errors


def asset_bytes(name: str) -> bytes:
    return f"payload for {name}".encode()


def contract_for(name: str, payload: bytes, source: str = "contract.json") -> dict:
    return {
        "downloadUrl": verifier.RELEASE_PREFIX + name,
        "sizeBytes": len(payload),
        "sha256": hashlib.sha256(payload).hexdigest(),
        "source": source,
    }


class FakeServer:
    """Serves bytes by exact URL, recording each request."""

    def __init__(self, served: dict[str, bytes]):
        self.served = served
        self.requests: list[str] = []

    def urlopen(self, request, timeout):
        url = request.full_url
        self.requests.append(url)
        if url not in self.served:
            raise OSError(f"404 for {url}")
        return io.BytesIO(self.served[url])


class VerifyUpdateCatalogSourcesTests(unittest.TestCase):
    def verify(self, documents: list[dict], served: dict[str, bytes], assets=None):
        contracts = [
            entry
            for document in documents
            for entry in verifier.hosted_contracts(document, document["source"])
        ]
        if assets is None:
            assets = [
                {"name": name, "size": size, "digest": f"sha256:{digest}"}
                for name, size, digest, _source, _url in contracts
            ]
        server = FakeServer(served)
        with mock.patch.object(verifier.urllib.request, "urlopen", server.urlopen):
            verified, errors = verifier.verify_contracts(contracts, assets)
        return verified, errors, server.requests

    def test_matching_public_bytes_are_downloaded_once_per_asset(self):
        payload = asset_bytes("tool.zip")
        document = contract_for("tool.zip", payload)
        url = document["downloadUrl"]
        verified, errors, requests = self.verify(
            [document, {**document, "source": "other.json"}], {url: payload}
        )
        self.assertEqual((verified, errors), (1, []))
        self.assertEqual(requests, [url])

    def test_matching_metadata_with_different_served_bytes_fails(self):
        payload = asset_bytes("tool.zip")
        document = contract_for("tool.zip", payload)
        tampered = bytes(reversed(payload))
        _verified, errors, _requests = self.verify(
            [document], {document["downloadUrl"]: tampered}
        )
        self.assertEqual(errors, ["runtime-bundles served bytes mismatch: tool.zip (contract.json)"])

    def test_truncated_oversized_and_unreachable_downloads_fail(self):
        payload = asset_bytes("tool.zip")
        document = contract_for("tool.zip", payload)
        url = document["downloadUrl"]
        for served in ({url: payload[:-1]}, {url: payload + b"extra"}):
            with self.subTest(size=len(served[url])):
                _verified, errors, _requests = self.verify([document], served)
                self.assertEqual(
                    errors, ["runtime-bundles served bytes mismatch: tool.zip (contract.json)"]
                )
        _verified, errors, _requests = self.verify([document], {})
        self.assertEqual(len(errors), 1)
        self.assertTrue(errors[0].startswith("runtime-bundles download failed: tool.zip"))

    def test_metadata_mismatch_is_reported_without_downloading(self):
        payload = asset_bytes("tool.zip")
        document = contract_for("tool.zip", payload)
        assets = [{"name": "tool.zip", "size": len(payload) + 1, "digest": "sha256:" + "0" * 64}]
        _verified, errors, requests = self.verify([document], {}, assets)
        self.assertEqual(errors, ["runtime-bundles identity mismatch: tool.zip (contract.json)"])
        self.assertEqual(requests, [])

    def test_oversized_response_is_not_read_past_the_recorded_size(self):
        reads: list[int] = []

        class Endless:
            def __enter__(self):
                return self

            def __exit__(self, *_):
                return False

            def read(self, size):
                reads.append(size)
                return b"x" * size

        with mock.patch.object(verifier, "DOWNLOAD_CHUNK_BYTES", 4), mock.patch.object(
            verifier.urllib.request, "urlopen", lambda request, timeout: Endless()
        ):
            size, _digest = verifier.downloaded_identity(verifier.RELEASE_PREFIX + "a", 10)
        self.assertEqual(size, 11)
        self.assertEqual(sum(reads), 11)


class PublishedCatalogReadBackTests(unittest.TestCase):
    """The publish workflow reads back the uploaded catalog through this path."""

    @staticmethod
    def package(catalog: bytes, signature: bytes) -> dict:
        def entry(name: str, payload: bytes) -> dict:
            return {
                "asset": name,
                "sizeBytes": len(payload),
                "sha256": hashlib.sha256(payload).hexdigest(),
            }

        return {
            "schemaVersion": 1,
            "sequence": 7,
            "catalog": entry("sgt-component-catalog-7.json", catalog),
            "signature": {**entry("sgt-component-catalog-7.sig", signature), "algorithm": "x"},
        }

    def read_back(self, package: dict, served: dict[str, bytes]):
        contracts = verifier.published_contracts(package, "packages.json")
        assets = [
            {"name": name, "size": size, "digest": f"sha256:{digest}"}
            for name, size, digest, _source, _url in contracts
        ]
        server = FakeServer(served)
        with mock.patch.object(verifier.urllib.request, "urlopen", server.urlopen):
            return verifier.verify_contracts(contracts, assets), server.requests

    def test_uploaded_catalog_and_signature_bytes_are_downloaded_and_hashed(self):
        catalog, signature = b'{"catalog":1}', b"signature"
        package = self.package(catalog, signature)
        catalog_url = verifier.RELEASE_PREFIX + "sgt-component-catalog-7.json"
        signature_url = verifier.RELEASE_PREFIX + "sgt-component-catalog-7.sig"
        (verified, errors), requests = self.read_back(
            package, {catalog_url: catalog, signature_url: signature}
        )
        self.assertEqual((verified, errors), (2, []))
        self.assertEqual(requests, [catalog_url, signature_url])

    def test_matching_release_metadata_cannot_hide_a_different_served_signature(self):
        catalog, signature = b'{"catalog":1}', b"signature"
        served = {
            verifier.RELEASE_PREFIX + "sgt-component-catalog-7.json": catalog,
            verifier.RELEASE_PREFIX + "sgt-component-catalog-7.sig": b"signaturX",
        }
        (_verified, errors), _requests = self.read_back(self.package(catalog, signature), served)
        self.assertEqual(
            errors,
            [
                "runtime-bundles served bytes mismatch: sgt-component-catalog-7.sig "
                "(packages.json signature)"
            ],
        )

    def test_package_entries_must_name_one_release_asset(self):
        package = self.package(b"c", b"s")
        package["signature"]["asset"] = "../escape.sig"
        with self.assertRaises(SystemExit):
            verifier.published_contracts(package, "packages.json")
        with self.assertRaises(SystemExit):
            verifier.published_contracts({"schemaVersion": 2}, "packages.json")



class CatalogSequenceTests(unittest.TestCase):
    PUBLISHED = [
        "sgt-component-catalog-v000013-0000000000000000.json",
        "sgt-component-catalog-v000014-e525a237ba182899.json",
        "sgt-component-catalog-v000014-e525a237ba182899.sig",
        "screen-recorder-5.6.0-fe797d091e2c4208.zip",
    ]

    def test_changed_contracts_at_a_published_sequence_are_rejected(self):
        errors = sequence_errors(
            14, "sgt-component-catalog-v000014-1111111111111111.json", self.PUBLISHED
        )
        self.assertEqual(len(errors), 1)
        self.assertIn("increment `sequence`", errors[0])

    def test_a_retry_of_the_published_catalog_passes(self):
        name = "sgt-component-catalog-v000014-e525a237ba182899.json"
        self.assertEqual(sequence_errors(14, name, self.PUBLISHED), [])

    def test_the_next_sequence_passes_and_an_older_one_is_rejected(self):
        self.assertEqual(
            sequence_errors(15, "sgt-component-catalog-v000015-2222222222222222.json", self.PUBLISHED),
            [],
        )
        older = sequence_errors(
            13, "sgt-component-catalog-v000013-3333333333333333.json", self.PUBLISHED
        )
        self.assertTrue(any("below the published sequence 14" in error for error in older))


if __name__ == "__main__":
    unittest.main()
