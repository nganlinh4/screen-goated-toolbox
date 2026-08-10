#!/usr/bin/env python3
"""Create deterministic, content-addressed Qwen3 CUDA runtime packs."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import zipfile
from pathlib import Path


COMPONENT_ID = "qwen3-cuda-runtime"
VC_COMPONENT_ID = "vc14-x64-runtime"
DEFAULT_VERSION = "2.7.1-cu128-abi2"
RUNTIME_DLL = "native/qwen3_runtime/dist/sgt_qwen3_runtime.dll"
RUNTIME_MANIFEST = "native/qwen3_runtime/dist/sgt_qwen3_runtime.manifest.json"
RUNTIME_FILES = (
    (RUNTIME_DLL, "bin/x64/sgt_qwen3_runtime.dll"),
    (
        "component-notices/qwen3-cuda-runtime/PYTORCH-LICENSE.txt",
        "licenses/PYTORCH-LICENSE.txt",
    ),
    (
        "component-notices/qwen3-cuda-runtime/PYTORCH-NOTICE.txt",
        "licenses/PYTORCH-NOTICE.txt",
    ),
    (
        "component-notices/qwen3-cuda-runtime/PYTORCH-LICENSES-BUNDLED.txt",
        "licenses/PYTORCH-LICENSES-BUNDLED.txt",
    ),
    (
        "component-notices/qwen3-cuda-runtime/CUDA-NOTICE.txt",
        "licenses/CUDA-NOTICE.txt",
    ),
    (
        "component-notices/qwen3-cuda-runtime/DNNL-LICENSE.txt",
        "licenses/DNNL-LICENSE.txt",
    ),
    (
        "component-notices/qwen3-cuda-runtime/DNNL-THIRD-PARTY-PROGRAMS.txt",
        "licenses/DNNL-THIRD-PARTY-PROGRAMS.txt",
    ),
)
LIBTORCH_NAME = "libtorch-win-shared-with-deps-2.7.1+cu128.zip"
LIBTORCH_SIZE = 3_214_239_381
LIBTORCH_SHA256 = "bdbf643d648e2bf9e8603472d6c6ff4bae5f79a49fe4776f215b4c45c90a7f19"
SELECTED_DLLS = (
    "asmjit.dll",
    "c10.dll",
    "c10_cuda.dll",
    "cublas64_12.dll",
    "cublasLt64_12.dll",
    "cudart64_12.dll",
    "cudnn64_9.dll",
    "cudnn_adv64_9.dll",
    "cudnn_cnn64_9.dll",
    "cudnn_engines_precompiled64_9.dll",
    "cudnn_engines_runtime_compiled64_9.dll",
    "cudnn_graph64_9.dll",
    "cudnn_heuristic64_9.dll",
    "cudnn_ops64_9.dll",
    "cufft64_11.dll",
    "cupti64_2025.1.0.dll",
    "cusolver64_11.dll",
    "cusparse64_12.dll",
    "fbgemm.dll",
    "libiomp5md.dll",
    "nvJitLink_120_0.dll",
    "nvrtc-builtins64_128.dll",
    "nvrtc64_120_0.dll",
    "torch_cpu.dll",
    "torch_cuda.dll",
    "uv.dll",
    "zlibwapi.dll",
)
OMITTED_DLLS = (
    "caffe2_nvrtc.dll",
    "cufftw64_11.dll",
    "curand64_10.dll",
    "cusolverMg64_11.dll",
    "libiompstubs5md.dll",
    "nvrtc64_120_0.alt.dll",
    "nvToolsExt64_1.dll",
    "torch.dll",
    "torch_global_deps.dll",
)
MARKERS = ("build-hash", "build-version")
FIXED_TIMESTAMP = (1980, 1, 1, 0, 0, 0)
LIBTORCH_PARTS = 2
MAX_RELEASE_ASSET_BYTES = 2_000_000_000


def hash_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(4 * 1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def require_x64_pe(prefix: bytes, label: str) -> None:
    if prefix[:2] != b"MZ" or len(prefix) < 64:
        raise RuntimeError(f"{label} is not a PE file")
    pe_offset = int.from_bytes(prefix[60:64], "little")
    if pe_offset + 6 > len(prefix) or prefix[pe_offset : pe_offset + 4] != b"PE\0\0":
        raise RuntimeError(f"{label} has an invalid PE header")
    if int.from_bytes(prefix[pe_offset + 4 : pe_offset + 6], "little") != 0x8664:
        raise RuntimeError(f"{label} is not Windows x64")


def zip_info(path: str, size: int) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(path, FIXED_TIMESTAMP)
    info.compress_type = zipfile.ZIP_DEFLATED
    info.create_system = 3
    info.external_attr = 0o100644 << 16
    info.file_size = size
    return info


def finalize_asset(
    temporary: Path, output: Path, name_prefix: str, repo: Path
) -> dict:
    asset_hash = hash_file(temporary)
    asset_name = f"{name_prefix}-{asset_hash[:16]}.zip"
    target = output / asset_name
    if target.exists():
        if target.stat().st_size != temporary.stat().st_size or hash_file(target) != asset_hash:
            raise RuntimeError(f"refusing to replace immutable asset {target}")
        temporary.unlink()
    else:
        temporary.replace(target)
    if target.stat().st_size >= MAX_RELEASE_ASSET_BYTES:
        raise RuntimeError(f"{asset_name} exceeds the safe GitHub release asset limit")
    return {
        "asset": asset_name,
        "assetPath": target.relative_to(repo).as_posix(),
        "sizeBytes": target.stat().st_size,
        "sha256": asset_hash,
    }


def runtime_asset(repo: Path, output: Path, version: str) -> tuple[dict, list[dict]]:
    runtime_path = repo / RUNTIME_DLL
    runtime = runtime_path.read_bytes()
    require_x64_pe(runtime[:4096], RUNTIME_DLL)
    manifest = json.loads((repo / RUNTIME_MANIFEST).read_text(encoding="utf-8"))
    runtime_hash = hashlib.sha256(runtime).hexdigest()
    if manifest != {"sha256": runtime_hash, "abi_version": 2, "size": len(runtime)}:
        raise RuntimeError("Qwen3 runtime manifest does not match the x64 ABI-2 DLL")

    temporary = output / f".{COMPONENT_ID}-{version}.zip.tmp"
    temporary.unlink(missing_ok=True)
    files = []
    with zipfile.ZipFile(
        temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for source_path, target_path in RUNTIME_FILES:
            data = (repo / source_path).read_bytes()
            if not data:
                raise RuntimeError(f"Qwen3 runtime component file is empty: {source_path}")
            archive.writestr(zip_info(target_path, len(data)), data)
            files.append(
                {
                    "archiveIndex": 0,
                    "archivePath": target_path,
                    "path": target_path,
                    "sizeBytes": len(data),
                    "sha256": hashlib.sha256(data).hexdigest(),
                }
            )
    asset = finalize_asset(
        temporary, output, f"{COMPONENT_ID}-{version}", repo
    )
    return asset, files


def inspect_libtorch(archive_path: Path) -> list[dict]:
    if archive_path.stat().st_size != LIBTORCH_SIZE:
        raise RuntimeError("libtorch archive size does not match the pinned release")
    if hash_file(archive_path) != LIBTORCH_SHA256:
        raise RuntimeError("libtorch archive checksum does not match the pinned release")
    expected_dlls = set(SELECTED_DLLS) | set(OMITTED_DLLS)
    with zipfile.ZipFile(archive_path) as archive:
        actual_dlls = {
            Path(info.filename).name
            for info in archive.infolist()
            if info.filename.startswith("libtorch/lib/")
            and info.filename.lower().endswith(".dll")
        }
        if actual_dlls != expected_dlls:
            raise RuntimeError("libtorch DLL set does not match the pinned source inventory")
        entries = []
        source_paths = [f"libtorch/lib/{name}" for name in SELECTED_DLLS]
        source_paths.extend(f"libtorch/{name}" for name in MARKERS)
        for source_path in source_paths:
            info = archive.getinfo(source_path)
            digest = hashlib.sha256()
            prefix = bytearray()
            with archive.open(info) as member:
                while chunk := member.read(4 * 1024 * 1024):
                    if len(prefix) < 4096:
                        prefix.extend(chunk[: 4096 - len(prefix)])
                    digest.update(chunk)
            if source_path.lower().endswith(".dll"):
                require_x64_pe(bytes(prefix), source_path)
                target_path = f"bin/x64/{Path(source_path).name}"
            else:
                target_path = f"metadata/{Path(source_path).name}"
            entries.append(
                {
                    "sourcePath": source_path,
                    "archivePath": target_path,
                    "path": target_path,
                    "sizeBytes": info.file_size,
                    "sha256": digest.hexdigest(),
                    "sourceCompressedBytes": info.compress_size,
                }
            )
    return entries


def partition_entries(entries: list[dict]) -> list[list[dict]]:
    parts: list[list[dict]] = [[] for _ in range(LIBTORCH_PARTS)]
    estimates = [0] * LIBTORCH_PARTS
    for entry in sorted(
        entries, key=lambda item: (-item["sourceCompressedBytes"], item["path"])
    ):
        index = min(range(LIBTORCH_PARTS), key=lambda candidate: estimates[candidate])
        parts[index].append(entry)
        estimates[index] += entry["sourceCompressedBytes"]
    return [sorted(part, key=lambda item: item["path"]) for part in parts]


def libtorch_assets(
    repo: Path, output: Path, version: str, archive_path: Path, entries: list[dict]
) -> tuple[list[dict], list[dict]]:
    assets = []
    owned = []
    with zipfile.ZipFile(archive_path) as source:
        for part_number, part in enumerate(partition_entries(entries), start=1):
            temporary = output / f".{COMPONENT_ID}-{version}-part{part_number}.zip.tmp"
            temporary.unlink(missing_ok=True)
            with zipfile.ZipFile(
                temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
            ) as target:
                for entry in part:
                    source_info = source.getinfo(entry["sourcePath"])
                    info = zip_info(entry["archivePath"], source_info.file_size)
                    with source.open(source_info) as input_file, target.open(info, "w") as output_file:
                        while chunk := input_file.read(4 * 1024 * 1024):
                            output_file.write(chunk)
            assets.append(
                finalize_asset(
                    temporary,
                    output,
                    f"qwen3-cuda-libtorch-{version}-part{part_number}",
                    repo,
                )
            )
            for entry in part:
                owned.append(
                    {
                        "archiveIndex": part_number,
                        "archivePath": entry["archivePath"],
                        "path": entry["path"],
                        "sizeBytes": entry["sizeBytes"],
                        "sha256": entry["sha256"],
                    }
                )
    return assets, owned


def require_matching_delivery(output: Path, descriptor: dict) -> None:
    delivery_path = output / "sgt_qwen3_runtime.delivery.json"
    if not delivery_path.is_file():
        raise RuntimeError(
            "verified Qwen3 delivery is missing; upload every immutable pack and "
            "run verify_qwen3_runtime_release.py"
        )
    delivery = json.loads(delivery_path.read_text(encoding="utf-8"))
    expected = json.loads(json.dumps(descriptor))
    for asset in expected["windows"]["components"][0]["assets"]:
        asset.pop("assetPath")
    if delivery != expected:
        raise RuntimeError("verified Qwen3 delivery does not match the pinned packs")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", default=DEFAULT_VERSION)
    parser.add_argument(
        "--output-dir", default="local-runtime-bundles/sgt_qwen3_runtime"
    )
    parser.add_argument("--libtorch-archive")
    parser.add_argument("--require-delivery", action="store_true")
    args = parser.parse_args()
    if not re.fullmatch(r"[a-z0-9._-]{1,80}", args.version):
        raise RuntimeError("Qwen3 runtime version is invalid")
    repo = Path(__file__).resolve().parents[1]
    output = (repo / args.output_dir).resolve()
    output.relative_to(repo)
    output.mkdir(parents=True, exist_ok=True)
    archive_path = (
        (repo / args.libtorch_archive).resolve()
        if args.libtorch_archive
        else output / LIBTORCH_NAME
    )
    archive_path.relative_to(repo)
    runtime, runtime_files = runtime_asset(repo, output, args.version)
    entries = inspect_libtorch(archive_path)
    libtorch, libtorch_files = libtorch_assets(
        repo, output, args.version, archive_path, entries
    )
    files = sorted([*runtime_files, *libtorch_files], key=lambda item: item["path"])
    component = {
        "id": COMPONENT_ID,
        "dependencies": [VC_COMPONENT_ID],
        "assets": [runtime, *libtorch],
        "unpackedSizeBytes": sum(file["sizeBytes"] for file in files),
        "files": files,
    }
    descriptor = {
        "schemaVersion": 1,
        "version": args.version,
        "windows": {"architecture": "x64", "components": [component]},
    }
    package_path = output / "sgt_qwen3_runtime.packages.json"
    package_path.write_text(json.dumps(descriptor, indent=2) + "\n", encoding="utf-8")
    if args.require_delivery:
        require_matching_delivery(output, descriptor)
    print(package_path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, KeyError, zipfile.BadZipFile) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
