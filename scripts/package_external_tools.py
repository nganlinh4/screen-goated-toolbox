#!/usr/bin/env python3
"""Prepare exact Windows external-tool and WebView2 delivery packages."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import urllib.request
import zipfile

from dev_cache_paths import require_repo_or_managed_cache
from pathlib import Path


FIXED_TIMESTAMP = (1980, 1, 1, 0, 0, 0)
OUTPUT_DEFAULT = "local-runtime-bundles/sgt_external_tools"
AUDIT_DEFAULT = "local-runtime-bundles/external-tool-audit"
RUNTIME_BUNDLES = (
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/"
    "sgt-runtime-bundles/"
)
YTDLP_VERSION = "2026.08.19"
YTDLP_SIZE = 17_840_399
YTDLP_SHA256 = "66674953fe251b89f4d08c5f0e35e0728679bd67ab3d7d05c0562af101dd3e7a"
YTDLP_SOURCE_URL = f"https://github.com/yt-dlp/yt-dlp/releases/download/{YTDLP_VERSION}/yt-dlp.exe"
DENO_VERSION = "2.9.5"
DENO_SIZE = 42_691_248
DENO_SHA256 = "171efab55ac6b9881fd53ee4c20f8bf3bb1340ffc618483746909014db12216a"
DENO_SOURCE_URL = f"https://github.com/denoland/deno/releases/download/v{DENO_VERSION}/deno-x86_64-pc-windows-msvc.zip"
DENO_EXE_SIZE = 97_408_288
DENO_EXE_SHA256 = "98f8c2a2d470e4ccb04c935c86ff8050817d877762aec5eaeeb9e409ccb3b9fd"
FFMPEG_VERSION = "n8.1.2-34-g9b6c8969e0-20260809"
FFMPEG_SOURCE_SIZE = 167_988_921
FFMPEG_SOURCE_SHA256 = "f713351576192ffdd6a321c6d567fd701b2fded6e078fc0f305303fa810c81c4"
FFMPEG_EXE_SIZE = 144_145_920
FFMPEG_EXE_SHA256 = "693debe65acf25b453be0978b77a6c64aa75ef23d35b009df195ea166cd912bf"
FFPROBE_EXE_SIZE = 143_939_584
FFPROBE_EXE_SHA256 = "8fd4598047b2f48ed803bf0080c0fffe482e2dd47239a5048881166a1b1e87b6"
FFMPEG_LICENSE_SIZE = 35_147
FFMPEG_LICENSE_SHA256 = "8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903"
FFMPEG_NOTICE_SIZE = 251
FFMPEG_NOTICE_SHA256 = "6519d80d8f89a6d61eeef7d513064fcfd80887485b314ef6aeff30529e1c5bfd"
FFMPEG_PACK_SIZE = 110_361_214
FFMPEG_PACK_SHA256 = "9281d896411bf231ed11113e89fb3d4d22c17fd5b012d6059046ab10ba6046ee"
WEBVIEW_VERSION = "1.3.251.23"
WEBVIEW_SIZE = 1_695_960
WEBVIEW_SHA256 = "8c4a80540b6bbcbef30a4e8c7d1ac504b6fc09db922b4acdfd85c9d5f6f1050e"
MICROSOFT_SUBJECT = "CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def host_version(repo: Path) -> str:
    cargo = (repo / "Cargo.toml").read_text(encoding="utf-8")
    package = cargo.split("[package]", 1)[1].split("[", 1)[0]
    match = re.search(r'^version\s*=\s*"([^"]+)"\s*$', package, re.MULTILINE)
    if match is None:
        raise RuntimeError("Cargo package version is unavailable")
    return match.group(1)


def exact_file(path: Path, size: int, digest: str, label: str) -> bytes:
    if not path.is_file():
        raise RuntimeError(f"{label} is missing: {path}")
    data = path.read_bytes()
    if len(data) != size or sha256(data) != digest:
        raise RuntimeError(f"{label} does not match the reviewed identity")
    return data


def ensure_exact_source(path: Path, url: str, size: int, digest: str, label: str) -> None:
    if path.is_file():
        exact_file(path, size, digest, label)
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "SGT-release-packager"})
    with urllib.request.urlopen(request, timeout=180) as response:
        data = response.read(size + 1)
    if len(data) != size or sha256(data) != digest:
        raise RuntimeError(f"downloaded {label} does not match the reviewed identity")
    temporary = path.with_name(f".{path.name}.download")
    temporary.write_bytes(data)
    os.replace(temporary, path)


def require_x64_pe(data: bytes, label: str) -> None:
    if len(data) < 70 or data[:2] != b"MZ":
        raise RuntimeError(f"{label} is not a PE executable")
    offset = struct.unpack_from("<I", data, 0x3C)[0]
    if offset > 1024 * 1024 or data[offset : offset + 4] != b"PE\0\0":
        raise RuntimeError(f"{label} has an invalid PE header")
    if struct.unpack_from("<H", data, offset + 4)[0] != 0x8664:
        raise RuntimeError(f"{label} is not Windows x64")


def require_windows_bootstrapper(data: bytes) -> None:
    if len(data) < 70 or data[:2] != b"MZ":
        raise RuntimeError("WebView2 bootstrapper is not PE")
    offset = struct.unpack_from("<I", data, 0x3C)[0]
    if (
        offset > 1024 * 1024
        or data[offset : offset + 4] != b"PE\0\0"
        or struct.unpack_from("<H", data, offset + 4)[0] not in {0x14C, 0x8664}
    ):
        raise RuntimeError("WebView2 bootstrapper has an invalid Windows PE header")


def zip_entry(archive: zipfile.ZipFile, path: str, data: bytes) -> None:
    info = zipfile.ZipInfo(path, FIXED_TIMESTAMP)
    info.compress_type = zipfile.ZIP_DEFLATED
    info.create_system = 3
    info.external_attr = 0o100644 << 16
    archive.writestr(info, data)


def file_record(path: str, archive_path: str, data: bytes) -> dict:
    return {
        "path": path,
        "archivePath": archive_path,
        "sizeBytes": len(data),
        "sha256": sha256(data),
    }


def ytdlp_component(audit: Path) -> dict:
    source = audit / f"yt-dlp-{YTDLP_VERSION}.exe"
    data = exact_file(source, YTDLP_SIZE, YTDLP_SHA256, "yt-dlp")
    require_x64_pe(data, "yt-dlp")
    return {
        "id": "yt-dlp-x64",
        "version": YTDLP_VERSION,
        "asset": "yt-dlp.exe",
        "sourceUrl": YTDLP_SOURCE_URL,
        "archiveFormat": "raw",
        "sizeBytes": len(data),
        "sha256": sha256(data),
        "unpackedSizeBytes": len(data),
        "files": [file_record("bin/x64/yt-dlp.exe", "yt-dlp.exe", data)],
    }


def deno_component(audit: Path) -> dict:
    name = f"deno-x86_64-pc-windows-msvc-v{DENO_VERSION}.zip"
    source = audit / name
    data = exact_file(source, DENO_SIZE, DENO_SHA256, "Deno archive")
    with zipfile.ZipFile(source) as archive:
        if archive.namelist() != ["deno.exe"]:
            raise RuntimeError("Deno archive inventory changed")
        executable = archive.read("deno.exe")
    if len(executable) != DENO_EXE_SIZE or sha256(executable) != DENO_EXE_SHA256:
        raise RuntimeError("Deno executable identity changed")
    require_x64_pe(executable, "Deno")
    return {
        "id": "deno-x64",
        "version": DENO_VERSION,
        "asset": "deno-x86_64-pc-windows-msvc.zip",
        "sourceUrl": DENO_SOURCE_URL,
        "archiveFormat": "zip",
        "sizeBytes": len(data),
        "sha256": sha256(data),
        "unpackedSizeBytes": len(executable),
        "files": [file_record("bin/x64/deno.exe", "deno.exe", executable)],
    }


def ffmpeg_component(audit: Path, output: Path) -> dict:
    source = audit / "ffmpeg-n8.1-2026.08.09-win64-gpl.zip"
    source_data = exact_file(
        source, FFMPEG_SOURCE_SIZE, FFMPEG_SOURCE_SHA256, "FFmpeg source archive"
    )
    with zipfile.ZipFile(source) as archive:
        selected: dict[str, bytes] = {}
        for entry in archive.infolist():
            for name in ("ffmpeg.exe", "ffprobe.exe", "LICENSE.txt"):
                if entry.filename.endswith(f"/{name}"):
                    if name in selected:
                        raise RuntimeError(f"FFmpeg archive repeats {name}")
                    selected[name] = archive.read(entry)
        if set(selected) != {"ffmpeg.exe", "ffprobe.exe", "LICENSE.txt"}:
            raise RuntimeError("FFmpeg source archive inventory changed")
    require_x64_pe(selected["ffmpeg.exe"], "FFmpeg")
    require_x64_pe(selected["ffprobe.exe"], "ffprobe")
    for name, expected_size, expected_hash in (
        ("ffmpeg.exe", FFMPEG_EXE_SIZE, FFMPEG_EXE_SHA256),
        ("ffprobe.exe", FFPROBE_EXE_SIZE, FFPROBE_EXE_SHA256),
        ("LICENSE.txt", FFMPEG_LICENSE_SIZE, FFMPEG_LICENSE_SHA256),
    ):
        if len(selected[name]) != expected_size or sha256(selected[name]) != expected_hash:
            raise RuntimeError(f"FFmpeg {name} identity changed")
    source_notice = (
        "Source: https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/"
        "ffmpeg-n8.1-latest-win64-gpl-8.1.zip\n"
        f"Source bytes: {len(source_data)}\n"
        f"Source SHA-256: {sha256(source_data)}\n"
        f"Build: {FFMPEG_VERSION}\n"
    ).encode()
    if len(source_notice) != FFMPEG_NOTICE_SIZE or sha256(source_notice) != FFMPEG_NOTICE_SHA256:
        raise RuntimeError("FFmpeg source notice identity changed")
    records = [
        ("bin/x64/ffmpeg.exe", selected["ffmpeg.exe"]),
        ("bin/x64/ffprobe.exe", selected["ffprobe.exe"]),
        ("licenses/LICENSE.txt", selected["LICENSE.txt"]),
        ("licenses/SOURCE.txt", source_notice),
    ]
    temporary = output / f".ffmpeg-x64-{FFMPEG_VERSION}.tmp"
    temporary.unlink(missing_ok=True)
    with zipfile.ZipFile(temporary, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for relative, payload in records:
            zip_entry(archive, relative, payload)
    package = temporary.read_bytes()
    digest = sha256(package)
    if len(package) != FFMPEG_PACK_SIZE or digest != FFMPEG_PACK_SHA256:
        raise RuntimeError("FFmpeg deterministic package identity changed")
    asset = f"ffmpeg-x64-{FFMPEG_VERSION}-{digest[:16]}.zip"
    target = output / asset
    if target.exists() and target.read_bytes() != package:
        raise RuntimeError(f"refusing to replace immutable asset {target}")
    if not target.exists():
        target.write_bytes(package)
    temporary.unlink()
    return {
        "id": "ffmpeg-x64",
        "version": FFMPEG_VERSION,
        "asset": asset,
        "archiveFormat": "zip",
        "sizeBytes": len(package),
        "sha256": digest,
        "unpackedSizeBytes": sum(len(data) for _, data in records),
        "files": [file_record(path, path, data) for path, data in records],
    }


def expected_ffmpeg_component() -> dict:
    files = [
        {
            "path": "bin/x64/ffmpeg.exe",
            "archivePath": "bin/x64/ffmpeg.exe",
            "sizeBytes": FFMPEG_EXE_SIZE,
            "sha256": FFMPEG_EXE_SHA256,
        },
        {
            "path": "bin/x64/ffprobe.exe",
            "archivePath": "bin/x64/ffprobe.exe",
            "sizeBytes": FFPROBE_EXE_SIZE,
            "sha256": FFPROBE_EXE_SHA256,
        },
        {
            "path": "licenses/LICENSE.txt",
            "archivePath": "licenses/LICENSE.txt",
            "sizeBytes": FFMPEG_LICENSE_SIZE,
            "sha256": FFMPEG_LICENSE_SHA256,
        },
        {
            "path": "licenses/SOURCE.txt",
            "archivePath": "licenses/SOURCE.txt",
            "sizeBytes": FFMPEG_NOTICE_SIZE,
            "sha256": FFMPEG_NOTICE_SHA256,
        },
    ]
    return {
        "id": "ffmpeg-x64",
        "version": FFMPEG_VERSION,
        "asset": f"ffmpeg-x64-{FFMPEG_VERSION}-{FFMPEG_PACK_SHA256[:16]}.zip",
        "archiveFormat": "zip",
        "sizeBytes": FFMPEG_PACK_SIZE,
        "sha256": FFMPEG_PACK_SHA256,
        "unpackedSizeBytes": sum(item["sizeBytes"] for item in files),
        "files": files,
    }


def verified_published_ffmpeg(output: Path, delivery: dict) -> dict:
    expected = expected_ffmpeg_component()
    delivered = next(
        (item for item in delivery.get("components", []) if item.get("id") == "ffmpeg-x64"),
        None,
    )
    if delivered is None or comparable(delivered) != expected:
        raise RuntimeError("verified FFmpeg delivery does not match the reviewed identity")
    expected_url = RUNTIME_BUNDLES + expected["asset"]
    if delivered.get("downloadUrl") != expected_url:
        raise RuntimeError("verified FFmpeg delivery URL is not immutable")
    request = urllib.request.Request(
        expected_url,
        headers={"User-Agent": "SGT-release-packager"},
    )
    with urllib.request.urlopen(request, timeout=180) as response:
        data = response.read(FFMPEG_PACK_SIZE + 1)
    if len(data) != FFMPEG_PACK_SIZE or sha256(data) != FFMPEG_PACK_SHA256:
        raise RuntimeError("published FFmpeg package identity changed")
    target = output / expected["asset"]
    if target.exists() and target.read_bytes() != data:
        raise RuntimeError(f"refusing to replace immutable asset {target}")
    if not target.exists():
        target.write_bytes(data)
    with zipfile.ZipFile(target) as archive:
        if archive.namelist() != [item["archivePath"] for item in expected["files"]]:
            raise RuntimeError("published FFmpeg package inventory changed")
        for item in expected["files"]:
            payload = archive.read(item["archivePath"])
            if len(payload) != item["sizeBytes"] or sha256(payload) != item["sha256"]:
                raise RuntimeError(f"published FFmpeg {item['path']} identity changed")
    return expected


def authenticode(path: Path) -> dict:
    script = (
        "$s=Get-AuthenticodeSignature -LiteralPath $env:SGT_SIGNATURE_FILE;"
        "$v=(Get-Item -LiteralPath $env:SGT_SIGNATURE_FILE).VersionInfo;"
        "[pscustomobject]@{Status=[string]$s.Status;Subject=$s.SignerCertificate.Subject;"
        "FileVersion=$v.FileVersion;CompanyName=$v.CompanyName}|ConvertTo-Json -Compress"
    )
    child_env = os.environ.copy()
    child_env["SGT_SIGNATURE_FILE"] = str(path)
    shell = shutil.which("pwsh") or shutil.which("powershell")
    if shell is None:
        raise RuntimeError("PowerShell is required to verify Authenticode")
    result = subprocess.run(
        [shell, "-NoProfile", "-NonInteractive", "-Command", script],
        check=True,
        capture_output=True,
        text=True,
        env=child_env,
    )
    return json.loads(result.stdout)


def webview_package(audit: Path, output: Path) -> dict:
    source = audit / "MicrosoftEdgeWebview2Setup-2026.08.10.exe"
    data = exact_file(source, WEBVIEW_SIZE, WEBVIEW_SHA256, "WebView2 bootstrapper")
    require_windows_bootstrapper(data)
    signature = authenticode(source)
    if (
        signature.get("Status") != "Valid"
        or signature.get("Subject") != MICROSOFT_SUBJECT
        or signature.get("FileVersion") != WEBVIEW_VERSION
        or signature.get("CompanyName") != "Microsoft Corporation"
    ):
        raise RuntimeError("WebView2 bootstrapper signature or version is unexpected")
    asset = f"webview2-bootstrapper-{WEBVIEW_VERSION}-{sha256(data)[:16]}.exe"
    target = output / asset
    if target.exists() and target.read_bytes() != data:
        raise RuntimeError(f"refusing to replace immutable asset {target}")
    if not target.exists():
        shutil.copyfile(source, target)
    return {
        "version": WEBVIEW_VERSION,
        "asset": asset,
        "sizeBytes": len(data),
        "sha256": sha256(data),
        "expectedPublisher": "Microsoft Corporation",
    }


def comparable(value: dict) -> dict:
    return {key: item for key, item in value.items() if key not in {"assetPath", "sourcePath", "sourceUrl", "downloadUrl"}}


def require_delivery(output: Path, descriptor: dict) -> None:
    path = output / "sgt_external_tools.delivery.json"
    if not path.is_file():
        raise RuntimeError("verified external-tool delivery is missing; upload the new immutable assets and run verify_external_tool_release.py")
    delivered = json.loads(path.read_text(encoding="utf-8"))
    if (
        delivered.get("schemaVersion") != 1
        or delivered.get("architecture") != "x64"
        or delivered.get("hostVersion") != descriptor["hostVersion"]
    ):
        raise RuntimeError("verified external-tool delivery header is invalid")
    expected_components = [comparable(item) for item in descriptor["components"]]
    actual_components = [comparable(item) for item in delivered.get("components", [])]
    if actual_components != expected_components or comparable(delivered.get("webview2Bootstrapper", {})) != comparable(descriptor["webview2Bootstrapper"]):
        raise RuntimeError("verified external-tool delivery differs from reviewed local artifacts")
    for expected, actual in zip(descriptor["components"], delivered["components"]):
        expected_url = expected.get("sourceUrl", RUNTIME_BUNDLES + expected["asset"])
        if actual.get("downloadUrl") != expected_url:
            raise RuntimeError(f"verified {expected['id']} delivery URL is not immutable")
    expected_webview_url = RUNTIME_BUNDLES + descriptor["webview2Bootstrapper"]["asset"]
    if delivered["webview2Bootstrapper"].get("downloadUrl") != expected_webview_url:
        raise RuntimeError("verified WebView2 delivery URL is not immutable")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--audit-dir", default=AUDIT_DEFAULT)
    parser.add_argument("--output-dir", default=OUTPUT_DEFAULT)
    parser.add_argument("--require-delivery", action="store_true")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    audit = require_repo_or_managed_cache(
        repo, repo / args.audit_dir, "external-tool audit"
    )
    output = require_repo_or_managed_cache(
        repo, repo / args.output_dir, "external-tool output"
    )
    output.mkdir(parents=True, exist_ok=True)
    delivery_path = output / "sgt_external_tools.delivery.json"
    delivery = (
        json.loads(delivery_path.read_text(encoding="utf-8"))
        if args.require_delivery and delivery_path.is_file()
        else None
    )
    if delivery is not None:
        ensure_exact_source(
            audit / f"yt-dlp-{YTDLP_VERSION}.exe",
            YTDLP_SOURCE_URL,
            YTDLP_SIZE,
            YTDLP_SHA256,
            "yt-dlp",
        )
        ensure_exact_source(
            audit / f"deno-x86_64-pc-windows-msvc-v{DENO_VERSION}.zip",
            DENO_SOURCE_URL,
            DENO_SIZE,
            DENO_SHA256,
            "Deno archive",
        )
        webview_url = delivery.get("webview2Bootstrapper", {}).get("downloadUrl")
        if not isinstance(webview_url, str) or not webview_url:
            raise RuntimeError("verified WebView2 delivery URL is missing")
        ensure_exact_source(
            audit / "MicrosoftEdgeWebview2Setup-2026.08.10.exe",
            webview_url,
            WEBVIEW_SIZE,
            WEBVIEW_SHA256,
            "WebView2 bootstrapper",
        )
    try:
        ffmpeg = ffmpeg_component(audit, output)
    except RuntimeError:
        if delivery is None:
            raise
        ffmpeg = verified_published_ffmpeg(output, delivery)
    descriptor = {
        "schemaVersion": 1,
        "hostVersion": host_version(repo),
        "architecture": "x64",
        "components": [
            ytdlp_component(audit),
            ffmpeg,
            deno_component(audit),
        ],
        "webview2Bootstrapper": webview_package(audit, output),
    }
    path = output / "sgt_external_tools.packages.json"
    path.write_text(json.dumps(descriptor, indent=2) + "\n", encoding="utf-8")
    if args.require_delivery:
        require_delivery(output, descriptor)
    print(path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, KeyError, zipfile.BadZipFile, subprocess.SubprocessError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
