#!/usr/bin/env python3
"""Create deterministic, independently removable Windows local-ASR packages."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import struct
import tempfile
import zipfile
from pathlib import Path

WORKER_VERSION = "1.0.0"
RUNTIME_VERSION = "1.24.2-directml-1.15.4"
FIXED_TIME = (1980, 1, 1, 0, 0, 0)
MACHINE_X64 = 0x8664
PARAKEET_LICENSE = (
    1_067,
    "e5c3ce5c06df58ffd79c7dbbc1b9bb754ee97bad25148a3c56bf48b349838dc1",
)

SOURCES = {
    "onnx": {
        "name": "microsoft.ml.onnxruntime.directml.1.24.2.nupkg",
        "sizeBytes": 12_411_398,
        "sha256": "c9b8adb96dfb5578097bea42a7d9b7ff8f300fb3c3a6f3052fe5b702628ab681",
        "entries": {
            "runtimes/win-x64/native/onnxruntime.dll": "bin/x64/onnxruntime.dll",
            "runtimes/win-x64/native/onnxruntime_providers_shared.dll": (
                "bin/x64/onnxruntime_providers_shared.dll"
            ),
            "LICENSE": "licenses/onnxruntime-LICENSE.txt",
            "ThirdPartyNotices.txt": (
                "licenses/onnxruntime-ThirdPartyNotices.txt"
            ),
        },
    },
    "directml": {
        "name": "microsoft.ai.directml.1.15.4.nupkg",
        "sizeBytes": 202_292_617,
        "sha256": "4e7cb7ddce8cf837a7a75dc029209b520ca0101470fcdf275c1f49736a3615b9",
        "entries": {
            "bin/x64-win/DirectML.dll": "bin/x64/DirectML.dll",
            "LICENSE-CODE.txt": "licenses/directml-LICENSE-CODE.txt",
            "LICENSE.txt": "licenses/directml-LICENSE.txt",
            "ThirdPartyNotices.txt": "licenses/directml-ThirdPartyNotices.txt",
        },
    },
}

RUNTIME_FILES = {
    "bin/x64/onnxruntime.dll": (
        17_270_304,
        "a2323bc49544645b911743052f1edce594e17df1e3423b71468c7386bc902f80",
    ),
    "bin/x64/onnxruntime_providers_shared.dll": (
        22_048,
        "8b33b30ac866c938aa3d946d4f92fc2ba70fff06ef45d5ce22e483f19ba2c896",
    ),
    "bin/x64/DirectML.dll": (
        18_527_776,
        "9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1",
    ),
    "licenses/onnxruntime-LICENSE.txt": (
        1_094,
        "c250d6278f0b47a6439fb7592b08b58a55eb9f535aa49a1db63211c3f982b674",
    ),
    "licenses/onnxruntime-ThirdPartyNotices.txt": (
        331_175,
        "fb0af774b4d7cffc5b9d046f2aaeade2f37df2f80abf8033c95dfffcc77a8866",
    ),
    "licenses/directml-LICENSE-CODE.txt": (
        1_093,
        "903df5512f7d02609fed0c780a9b704f5a3eeb6e4d84ebe42a29845c81899a3c",
    ),
    "licenses/directml-LICENSE.txt": (
        10_439,
        "a05138e3a085ff60a44881eedfa58dccb03ecc1d7b1f6ae888418e8c2fec4b8d",
    ),
    "licenses/directml-ThirdPartyNotices.txt": (
        4_577,
        "2c95795c13ff48a58b6ed916f37901c23d964b5d9d601af422f17ad2172e7950",
    ),
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_file(path: Path, expected_size: int, expected_sha: str) -> None:
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"required regular file is missing: {path}")
    if path.stat().st_size != expected_size or sha256(path) != expected_sha:
        raise ValueError(f"file identity mismatch: {path}")


def validate_x64_pe(path: Path) -> None:
    with path.open("rb") as stream:
        dos = stream.read(64)
        if len(dos) != 64 or dos[:2] != b"MZ":
            raise ValueError(f"not a PE executable: {path}")
        offset = struct.unpack_from("<I", dos, 0x3C)[0]
        stream.seek(offset)
        prefix = stream.read(6)
    if len(prefix) != 6 or prefix[:4] != b"PE\0\0":
        raise ValueError(f"invalid PE header: {path}")
    if struct.unpack_from("<H", prefix, 4)[0] != MACHINE_X64:
        raise ValueError(f"not an x64 PE file: {path}")


def extract_runtime(onnx: Path, directml: Path, output: Path) -> None:
    packages = {"onnx": onnx, "directml": directml}
    for key, archive in packages.items():
        contract = SOURCES[key]
        validate_file(archive, contract["sizeBytes"], contract["sha256"])
        with zipfile.ZipFile(archive) as source:
            for source_name, relative in contract["entries"].items():
                target = output / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                with source.open(source_name) as reader, target.open("xb") as writer:
                    shutil.copyfileobj(reader, writer, 1024 * 1024)
    for relative, (size, digest) in RUNTIME_FILES.items():
        path = output / relative
        validate_file(path, size, digest)
        if relative.startswith("bin/x64/"):
            validate_x64_pe(path)


def deterministic_zip(output: Path, files: list[tuple[str, Path]]) -> None:
    with zipfile.ZipFile(
        output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for relative, source in sorted(files):
            info = zipfile.ZipInfo(relative, FIXED_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            with source.open("rb") as reader, archive.open(info, "w") as writer:
                shutil.copyfileobj(reader, writer, 1024 * 1024)


def file_records(files: list[tuple[str, Path]]) -> list[dict[str, object]]:
    return [
        {
            "path": relative,
            "sizeBytes": source.stat().st_size,
            "sha256": sha256(source),
        }
        for relative, source in sorted(files)
    ]


def package_component(
    output_dir: Path,
    component_id: str,
    version: str,
    files: list[tuple[str, Path]],
) -> dict[str, object]:
    temporary = output_dir / f".{component_id}.zip.tmp"
    check = output_dir / f".{component_id}.zip.check"
    for path in (temporary, check):
        path.unlink(missing_ok=True)
    deterministic_zip(temporary, files)
    deterministic_zip(check, files)
    if sha256(temporary) != sha256(check):
        raise ValueError(f"nondeterministic archive output for {component_id}")
    check.unlink()
    digest = sha256(temporary)
    asset = f"{component_id}-{version}-{digest[:16]}.zip"
    target = output_dir / asset
    if target.exists() and sha256(target) != digest:
        raise ValueError(f"refusing to replace differing package: {target}")
    temporary.replace(target)
    records = file_records(files)
    return {
        "id": component_id,
        "version": version,
        "asset": asset,
        "sizeBytes": target.stat().st_size,
        "sha256": digest,
        "unpackedSizeBytes": sum(record["sizeBytes"] for record in records),
        "files": records,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--worker-exe", type=Path, required=True)
    parser.add_argument("--parakeet-license", type=Path, required=True)
    parser.add_argument("--onnx-package", type=Path, required=True)
    parser.add_argument("--directml-package", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    validate_x64_pe(args.worker_exe)
    validate_file(args.parakeet_license, *PARAKEET_LICENSE)

    with tempfile.TemporaryDirectory(prefix="sgt-local-asr-") as temporary:
        runtime_root = Path(temporary)
        extract_runtime(args.onnx_package, args.directml_package, runtime_root)
        worker = package_component(
            args.output_dir,
            "local-asr-worker",
            WORKER_VERSION,
            [
                ("bin/x64/sgt-local-asr-worker.exe", args.worker_exe),
                ("licenses/parakeet-rs-LICENSE.txt", args.parakeet_license),
            ],
        )
        runtime = package_component(
            args.output_dir,
            "onnx-directml-runtime",
            RUNTIME_VERSION,
            [(relative, runtime_root / relative) for relative in RUNTIME_FILES],
        )

    manifest = {
        "schemaVersion": 1,
        "architecture": "x64",
        "components": [worker, runtime],
        "sources": [
            {
                "name": contract["name"],
                "sizeBytes": contract["sizeBytes"],
                "sha256": contract["sha256"],
            }
            for contract in SOURCES.values()
        ],
    }
    manifest_path = args.output_dir / "sgt_local_asr.packages.json"
    manifest_path.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(manifest_path)
    for component in manifest["components"]:
        print(
            f"{component['asset']} {component['sizeBytes']} {component['sha256']}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
