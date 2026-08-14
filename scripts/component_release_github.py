"""GitHub release transport with mandatory byte read-back."""

from __future__ import annotations

import hashlib
import http.client
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
from typing import Any
from urllib import error as url_error
from urllib import request as url_request

STAGING_TAG = "sgt-runtime-staging"
PUBLIC_PROPAGATION_TIMEOUT_SECONDS = 600.0
PUBLIC_PROPAGATION_POLL_SECONDS = 5.0
PUBLIC_REQUEST_TIMEOUT_SECONDS = 30.0
PUBLIC_READ_CHUNK_BYTES = 1024 * 1024


def run(args: list[str], *, stdout: Any = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        check=True,
        text=True,
        stdout=subprocess.PIPE if stdout is None else stdout,
        stderr=subprocess.PIPE,
    )


def gh(*args: str, stdout: Any = None) -> subprocess.CompletedProcess[str]:
    return run(["gh", *args], stdout=stdout)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def public_download_url(repository: str, tag: str, name: str) -> str:
    return f"https://github.com/{repository}/releases/download/{tag}/{name}"


def _public_response_matches(response: Any, expected_size: int, expected_sha256: str) -> str | None:
    status = getattr(response, "status", None)
    if status != 200:
        return f"HTTP {status}" if isinstance(status, int) else "missing HTTP status"
    digest = hashlib.sha256()
    size = 0
    while chunk := response.read(PUBLIC_READ_CHUNK_BYTES):
        size += len(chunk)
        if size > expected_size:
            return "size mismatch"
        digest.update(chunk)
    if size != expected_size:
        return "size mismatch"
    if digest.hexdigest() != expected_sha256:
        return "SHA-256 mismatch"
    return None


def verify_public_download(
    url: str,
    label: str,
    expected_size: int,
    expected_sha256: str,
    *,
    timeout_seconds: float = PUBLIC_PROPAGATION_TIMEOUT_SECONDS,
    poll_seconds: float = PUBLIC_PROPAGATION_POLL_SECONDS,
) -> None:
    if timeout_seconds <= 0 or poll_seconds < 0:
        raise ValueError("public propagation timing must be bounded and non-negative")
    safe_label = label.partition("?")[0].partition("#")[0]
    deadline = time.monotonic() + timeout_seconds
    attempts = 0
    last_result = "not requested"
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        attempts += 1
        request = url_request.Request(
            url,
            headers={
                "Accept": "application/octet-stream",
                "Cache-Control": "no-cache",
                "User-Agent": "SGT-component-release-verifier",
            },
        )
        try:
            request_timeout = min(PUBLIC_REQUEST_TIMEOUT_SECONDS, remaining)
            with url_request.urlopen(request, timeout=request_timeout) as response:
                mismatch = _public_response_matches(
                    response, expected_size, expected_sha256
                )
            if mismatch is None:
                return
            last_result = mismatch
        except url_error.HTTPError as error:
            last_result = f"HTTP {error.code}"
            error.close()
        except (
            url_error.URLError,
            TimeoutError,
            OSError,
            http.client.HTTPException,
        ) as error:
            last_result = f"network error ({type(error).__name__})"

        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        time.sleep(min(poll_seconds, remaining))

    duration = f"{timeout_seconds:g}s"
    raise RuntimeError(
        f"public release URL did not propagate for {safe_label} within {duration} "
        f"after {attempts} attempts; last result: {last_result}"
    )


def verify_public_release_asset(
    repository: str,
    tag: str,
    name: str,
    expected_size: int,
    expected_sha256: str,
) -> None:
    verify_public_download(
        public_download_url(repository, tag, name),
        f"{tag}/{name}",
        expected_size,
        expected_sha256,
    )


def release(repository: str, tag: str) -> dict[str, Any]:
    result = gh("api", f"repos/{repository}/releases/tags/{tag}")
    value = json.loads(result.stdout)
    if not isinstance(value, dict):
        raise ValueError(f"GitHub returned an invalid release for {tag}")
    return value


def release_assets(repository: str, tag: str) -> dict[str, dict[str, Any]]:
    value = release(repository, tag)
    release_id = value.get("id")
    if not isinstance(release_id, int):
        raise ValueError(f"GitHub release {tag} has no numeric id")
    response = gh(
        "api",
        "--paginate",
        "--slurp",
        f"repos/{repository}/releases/{release_id}/assets?per_page=100",
    )
    pages = json.loads(response.stdout)
    if not isinstance(pages, list):
        raise ValueError(f"GitHub release {tag} has no asset pages")
    assets = [asset for page in pages for asset in page]
    return {
        asset["name"]: asset
        for asset in assets
        if isinstance(asset, dict) and isinstance(asset.get("name"), str)
    }


def ensure_staging_release(repository: str) -> None:
    result = subprocess.run(
        ["gh", "release", "view", STAGING_TAG, "--repo", repository],
        text=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    if result.returncode == 0:
        return
    gh(
        "release",
        "create",
        STAGING_TAG,
        "--repo",
        repository,
        "--prerelease",
        "--title",
        "SGT runtime staging",
        "--notes",
        "Mutable development candidates only. Released SGT builds never use this tag.",
    )


def download_asset(repository: str, tag: str, name: str, output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="sgt-release-download-") as directory:
        gh(
            "release",
            "download",
            tag,
            "--repo",
            repository,
            "--pattern",
            name,
            "--dir",
            directory,
        )
        downloaded = Path(directory, name)
        if not downloaded.is_file():
            raise RuntimeError(f"GitHub did not return {tag}/{name}")
        shutil.move(downloaded, output)


def verify_remote(
    repository: str,
    tag: str,
    name: str,
    expected_size: int,
    expected_sha256: str,
) -> None:
    asset = release_assets(repository, tag).get(name)
    if not asset:
        raise RuntimeError(f"published asset is missing: {tag}/{name}")
    if asset.get("size") != expected_size:
        raise RuntimeError(f"published asset has the wrong size: {tag}/{name}")
    remote_digest = asset.get("digest")
    if remote_digest and remote_digest != f"sha256:{expected_sha256}":
        raise RuntimeError(f"GitHub digest mismatch: {tag}/{name}")
    with tempfile.TemporaryDirectory(prefix="sgt-release-verify-") as directory:
        target = Path(directory, name)
        download_asset(repository, tag, name, target)
        if target.stat().st_size != expected_size or sha256(target) != expected_sha256:
            raise RuntimeError(f"published bytes failed read-back: {tag}/{name}")
    if tag == STAGING_TAG:
        verify_public_release_asset(
            repository, tag, name, expected_size, expected_sha256
        )
