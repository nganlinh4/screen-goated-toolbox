#!/usr/bin/env python3
"""Read back exact external-tool assets and emit host delivery metadata."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tempfile
import urllib.request
import zipfile
from pathlib import Path

from package_external_tools import authenticode


API_URL = "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/releases/tags/sgt-runtime-bundles"


def request(url: str):
    return urllib.request.urlopen(
        urllib.request.Request(
            url,
            headers={
                "Accept": "application/vnd.github+json",
                "User-Agent": "SGT-release-verifier",
            },
        ),
        timeout=180,
    )


def request_json(url: str) -> dict:
    with request(url) as response:
        data = response.read(4 * 1024 * 1024 + 1)
    if len(data) > 4 * 1024 * 1024:
        raise RuntimeError("release response exceeds its limit")
    return json.loads(data)


def download_exact(url: str, size: int, digest: str) -> bytes:
    with request(url) as response:
        header = response.headers.get("Content-Length")
        if header is not None and int(header) != size:
            raise RuntimeError(f"remote Content-Length mismatch for {url}")
        data = response.read(size + 1)
    if len(data) != size or hashlib.sha256(data).hexdigest() != digest:
        raise RuntimeError(f"remote bytes do not match the reviewed identity: {url}")
    return data


def verify_inventory(component: dict, data: bytes) -> None:
    files = component["files"]
    if component["archiveFormat"] == "raw":
        if len(files) != 1:
            raise RuntimeError("raw external tool has an invalid inventory")
        payloads = {files[0]["archivePath"]: data}
    else:
        with tempfile.TemporaryFile() as temporary:
            temporary.write(data)
            temporary.seek(0)
            with zipfile.ZipFile(temporary) as archive:
                names = archive.namelist()
                expected = [file["archivePath"] for file in files]
                if len(names) != len(expected) or set(names) != set(expected):
                    raise RuntimeError(f"remote {component['id']} inventory changed")
                payloads = {name: archive.read(name) for name in names}
    total = 0
    for file in files:
        payload = payloads[file["archivePath"]]
        if (
            len(payload) != file["sizeBytes"]
            or hashlib.sha256(payload).hexdigest() != file["sha256"]
        ):
            raise RuntimeError(f"remote {component['id']} file identity changed")
        total += len(payload)
    if total != component["unpackedSizeBytes"]:
        raise RuntimeError(f"remote {component['id']} unpacked size changed")


def remote_asset(component: dict, assets: dict) -> tuple[str, bytes]:
    source_url = component.get("sourceUrl")
    if source_url:
        data = download_exact(source_url, component["sizeBytes"], component["sha256"])
        return source_url, data
    remote = assets.get(component["asset"])
    if remote is None:
        raise RuntimeError(f"runtime-bundles release is missing {component['asset']}")
    if remote.get("size") != component["sizeBytes"]:
        raise RuntimeError(f"release size mismatch for {component['asset']}")
    url = remote["browser_download_url"]
    return url, download_exact(url, component["sizeBytes"], component["sha256"])


def delivered_component(component: dict, assets: dict) -> dict:
    url, data = remote_asset(component, assets)
    verify_inventory(component, data)
    result = {
        key: value
        for key, value in component.items()
        if key not in {"assetPath", "sourcePath", "sourceUrl"}
    }
    result["downloadUrl"] = url
    return result


def delivered_webview(webview: dict, assets: dict) -> dict:
    remote = assets.get(webview["asset"])
    if remote is None:
        raise RuntimeError(f"runtime-bundles release is missing {webview['asset']}")
    if remote.get("size") != webview["sizeBytes"]:
        raise RuntimeError("WebView2 bootstrapper release size mismatch")
    url = remote["browser_download_url"]
    data = download_exact(url, webview["sizeBytes"], webview["sha256"])
    with tempfile.TemporaryDirectory(prefix="sgt-webview-verify-") as temporary:
        path = Path(temporary) / webview["asset"]
        path.write_bytes(data)
        signature = authenticode(path)
    if (
        signature.get("Status") != "Valid"
        or signature.get("CompanyName") != webview["expectedPublisher"]
        or signature.get("FileVersion") != webview["version"]
        or not signature.get("Subject", "").startswith(
            f"CN={webview['expectedPublisher']}, O={webview['expectedPublisher']},"
        )
    ):
        raise RuntimeError("published WebView2 bootstrapper publisher or version changed")
    result = {
        key: value for key, value in webview.items() if key != "assetPath"
    }
    result["downloadUrl"] = url
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--packages",
        default="local-runtime-bundles/sgt_external_tools/sgt_external_tools.packages.json",
    )
    parser.add_argument(
        "--output",
        default="local-runtime-bundles/sgt_external_tools/sgt_external_tools.delivery.json",
    )
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    packages_path = (repo / args.packages).resolve()
    output_path = (repo / args.output).resolve()
    packages_path.relative_to(repo)
    output_path.relative_to(repo)
    packages = json.loads(packages_path.read_text(encoding="utf-8"))
    release = request_json(API_URL)
    assets = {asset["name"]: asset for asset in release.get("assets", [])}
    required_release_assets = [
        component["asset"]
        for component in packages["components"]
        if "sourceUrl" not in component
    ] + [packages["webview2Bootstrapper"]["asset"]]
    missing = [name for name in required_release_assets if name not in assets]
    if missing:
        raise RuntimeError(
            "runtime-bundles release is missing required assets: " + ", ".join(missing)
        )
    delivery = {
        "schemaVersion": 1,
        "hostVersion": packages["hostVersion"],
        "architecture": "x64",
        "components": [
            delivered_component(component, assets)
            for component in packages["components"]
        ],
        "webview2Bootstrapper": delivered_webview(
            packages["webview2Bootstrapper"], assets
        ),
    }
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(delivery, indent=2) + "\n", encoding="utf-8")
    print(output_path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (
        OSError,
        RuntimeError,
        ValueError,
        KeyError,
        json.JSONDecodeError,
        zipfile.BadZipFile,
    ) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
