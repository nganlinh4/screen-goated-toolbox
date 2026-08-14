import hashlib
import importlib.util
from io import BytesIO
from pathlib import Path
import unittest
from unittest import mock
from urllib import error as url_error


SCRIPT = Path(__file__).resolve().parents[1] / "component_release_github.py"
SPEC = importlib.util.spec_from_file_location("component_release_github_tests", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class FakeResponse:
    def __init__(self, payload: bytes, status: int = 200):
        self.status = status
        self.stream = BytesIO(payload)

    def __enter__(self):
        return self

    def __exit__(self, _type, _value, _traceback):
        return False

    def read(self, size: int) -> bytes:
        return self.stream.read(size)


class PublicDownloadVerificationTests(unittest.TestCase):
    def test_public_url_is_the_host_release_download_url(self):
        self.assertEqual(
            MODULE.public_download_url("owner/repo", "staging", "asset.zip"),
            "https://github.com/owner/repo/releases/download/staging/asset.zip",
        )

    def test_public_verification_retries_404_then_hashes_expected_bytes(self):
        payload = b"published bytes"
        digest = hashlib.sha256(payload).hexdigest()
        url = "https://github.com/owner/repo/releases/download/staging/asset.zip"
        seen_urls = []
        responses = [
            url_error.HTTPError(
                f"{url}?token=private",
                404,
                "private response body",
                {},
                BytesIO(b"private response body"),
            ),
            FakeResponse(payload),
        ]

        def open_request(request, timeout):
            self.assertGreater(timeout, 0)
            seen_urls.append(request.full_url)
            value = responses.pop(0)
            if isinstance(value, BaseException):
                raise value
            return value

        with (
            mock.patch.object(MODULE.url_request, "urlopen", side_effect=open_request),
            mock.patch.object(MODULE.time, "monotonic", return_value=0.0),
            mock.patch.object(MODULE.time, "sleep") as sleep,
        ):
            MODULE.verify_public_download(
                url,
                "staging/asset.zip",
                len(payload),
                digest,
                timeout_seconds=10,
                poll_seconds=1,
            )

        self.assertEqual(seen_urls, [url, url])
        sleep.assert_called_once_with(1)

    def test_public_verification_retries_digest_mismatch(self):
        payload = b"expected"
        digest = hashlib.sha256(payload).hexdigest()
        with (
            mock.patch.object(
                MODULE.url_request,
                "urlopen",
                side_effect=[FakeResponse(b"mismatch"), FakeResponse(payload)],
            ) as opener,
            mock.patch.object(MODULE.time, "monotonic", return_value=0.0),
            mock.patch.object(MODULE.time, "sleep"),
        ):
            MODULE.verify_public_download(
                "https://example.invalid/asset",
                "staging/asset.zip",
                len(payload),
                digest,
                timeout_seconds=10,
                poll_seconds=0,
            )
        self.assertEqual(opener.call_count, 2)

    def test_public_verification_timeout_is_useful_and_redacted(self):
        private_url = "https://example.invalid/asset?token=do-not-log"
        error = url_error.HTTPError(
            private_url,
            404,
            "private response body",
            {},
            BytesIO(b"private response body"),
        )
        with (
            mock.patch.object(MODULE.url_request, "urlopen", side_effect=error),
            mock.patch.object(MODULE.time, "monotonic", side_effect=[0.0, 0.0, 1.0]),
            mock.patch.object(MODULE.time, "sleep") as sleep,
        ):
            with self.assertRaisesRegex(RuntimeError, "staging/asset.zip") as raised:
                MODULE.verify_public_download(
                    private_url,
                    "staging/asset.zip?token=label-secret",
                    1,
                    "0" * 64,
                    timeout_seconds=1,
                    poll_seconds=0.5,
                )

        message = str(raised.exception)
        self.assertIn("HTTP 404", message)
        self.assertIn("after 1 attempts", message)
        self.assertNotIn("do-not-log", message)
        self.assertNotIn("label-secret", message)
        self.assertNotIn("private response body", message)
        sleep.assert_not_called()

    def test_release_asset_verifier_uses_exact_public_url(self):
        with mock.patch.object(MODULE, "verify_public_download") as verify:
            MODULE.verify_public_release_asset(
                "owner/repo", "staging", "asset.zip", 10, "1" * 64
            )
        verify.assert_called_once_with(
            "https://github.com/owner/repo/releases/download/staging/asset.zip",
            "staging/asset.zip",
            10,
            "1" * 64,
        )

    def test_staging_remote_verification_keeps_authenticated_readback_first(self):
        payload = b"authenticated bytes"
        digest = hashlib.sha256(payload).hexdigest()
        calls = []

        def download(_repository, _tag, _name, output):
            calls.append("authenticated")
            output.write_bytes(payload)

        def public(*_args):
            calls.append("public")

        with (
            mock.patch.object(
                MODULE,
                "release_assets",
                return_value={
                    "asset.zip": {
                        "size": len(payload),
                        "digest": f"sha256:{digest}",
                    }
                },
            ),
            mock.patch.object(MODULE, "download_asset", side_effect=download),
            mock.patch.object(
                MODULE, "verify_public_release_asset", side_effect=public
            ),
        ):
            MODULE.verify_remote(
                "owner/repo",
                MODULE.STAGING_TAG,
                "asset.zip",
                len(payload),
                digest,
            )

        self.assertEqual(calls, ["authenticated", "public"])


if __name__ == "__main__":
    unittest.main()
