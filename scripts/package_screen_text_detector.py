#!/usr/bin/env python3
"""Build the deterministic Screen Translate detector worker/model component."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import struct
import subprocess
import sys
import zipfile
from pathlib import Path

import onnx
from onnx import TensorProto, helper


FIXED_TIMESTAMP = (1980, 1, 1, 0, 0, 0)
MACHINE_X64 = 0x8664
COMPONENT_ID = "screen-text-detector"
RELEASE_PREFIX = (
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/"
    "sgt-runtime-bundles/"
)
LICENSE_PREFIXES = ("license", "licence", "copying", "notice", "copyright")
FIRST_PARTY = {
    "sgt-screen-text-detector-worker",
    "sgt-screen-text-detector-protocol",
}
DETECTOR_SHA256 = "39133f78eb2cac057bfd4d66ec08e94f8cf5e921da9075bcf2bd68cfbbb91b36"
DETECTOR_ONNX_SHA256 = "a431985659dc921974177a95adcfbb90fd9e51989a5e04d70d0b75f597b6e61d"
MODEL_README_SHA256 = "951c155673ac6c15feb85857c9024a62266e5956d15319ce578a8f508f399d66"
RECOGNIZERS = (
    (
        "unified",
        "PP-OCRv6_small_rec_onnx",
        "5435fd747c9e0efe15a96d0b378d5bd157e9492ed8fd80edf08f30d02fa24634",
        "ab078671bb49f06228eadccd34f1bb501e157f7a047095ffb943ba81512c77d1",
        False,
    ),
    (
        "hangul",
        "korean_PP-OCRv5_mobile_rec_onnx",
        "92f0b7785e64fc9090106a241cf4c1eb97472824558272751b88a2a4476d3a08",
        "f757fa1c40e99edcf27e9cce879b93eb2a51fa46f5ef39095689b8c37dd75998",
        False,
    ),
    (
        "cyrillic",
        "cyrillic_PP-OCRv5_mobile_rec_onnx",
        "5371ee1ddaa7983cc62d0818d99e982b6804638c85e4f960d59a574094e172e5",
        "5c76cc91fa98410178a09f498db10050d0ec1634a660053d3005ab7be581f501",
        False,
    ),
    (
        "arabic",
        "arabic_PP-OCRv5_mobile_rec_onnx",
        "799113ebf267fbe742deb99eb36e8d42c9ddc5291ceacf92add41b4d52a59110",
        "21368419e6c016c31db55d316d59e11c128e1913e6e6fe10287084710043d3a6",
        True,
    ),
    (
        "devanagari",
        "devanagari_PP-OCRv5_mobile_rec_onnx",
        "cb789212ce96c69d3e74728ae4309d179281d68cb3945d0616b67cafab41c986",
        "9bd172dd26440c8ce94d1cde5d5baea6aefdc7cf3c5c8492e0beedef656d4e54",
        False,
    ),
    (
        "thai",
        "th_PP-OCRv5_mobile_rec_onnx",
        "27618be66018f8598ac0a526a593f9f1cebf794e7eded93428e8fb016e537f5f",
        "f6ba7fefc38ca1ff398ddafa75d67d16e0b3757c4e6c833adffee98a981766c9",
        False,
    ),
    (
        "greek",
        "el_PP-OCRv5_mobile_rec_onnx",
        "2acf17fcaea2bc81b878e311e6263b8885f48bb03796f75f9f30ed3242bbaa6d",
        "17d85b2fe2d2f24cd4ab07bcbc33e0c126859b956ced36e281dc65e2d0c1f0bf",
        False,
    ),
    (
        "tamil",
        "ta_PP-OCRv5_mobile_rec_onnx",
        "c6d2b682d2a0ea4cb1fccdba295976f93fd439964d16cdc666cadef531accbee",
        "88a28f5a1bb30cabe38a0985cb5e6619fa4f0c7c78e57a08274674228c5219a6",
        False,
    ),
    (
        "telugu",
        "te_PP-OCRv5_mobile_rec_onnx",
        "8238bfc46d4cffe720ed6706e3842802467343497428693ff2bfb4e6b3caa36b",
        "acebbe53f1831bf28ddfed75aedf58225d7aa5d09100c1d5a9140a2a53b137ce",
        False,
    ),
)

RECOGNIZER_COVERAGE = {
    "hangul": ((0x1100, 0x11FF), (0x3130, 0x318F), (0x3400, 0x4DBF), (0x4E00, 0x9FFF), (0xA960, 0xA97F), (0xAC00, 0xD7FF)),
    "cyrillic": ((0x0400, 0x052F), (0x2DE0, 0x2DFF), (0xA640, 0xA69F)),
    "arabic": ((0x0600, 0x06FF), (0x0750, 0x077F), (0x08A0, 0x08FF), (0xFB50, 0xFDFF), (0xFE70, 0xFEFF)),
    "devanagari": ((0x0900, 0x097F), (0xA8E0, 0xA8FF)),
    "thai": ((0x0E00, 0x0E7F),),
    "greek": ((0x0370, 0x03FF), (0x1F00, 0x1FFF)),
    "tamil": ((0x0B80, 0x0BFF),),
    "telugu": ((0x0C00, 0x0C7F),),
}

RECOGNIZER_ROUTING = {
    "hangul": ((0x1100, 0x11FF), (0x3130, 0x318F), (0xA960, 0xA97F), (0xAC00, 0xD7FF)),
    **{identifier: coverage for identifier, coverage in RECOGNIZER_COVERAGE.items() if identifier != "hangul"},
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def manifest_asset_path(repo: Path, target: Path) -> str:
    try:
        return target.relative_to(repo).as_posix()
    except ValueError:
        return str(target)


def worker_version(manifest: Path) -> str:
    match = re.search(
        r'^version\s*=\s*"([^"]+)"',
        manifest.read_text(encoding="utf-8"),
        re.MULTILINE,
    )
    if not match:
        raise RuntimeError("detector worker version is missing")
    return match.group(1)


def validate_x64_pe(path: Path) -> None:
    if not path.is_file() or path.is_symlink():
        raise RuntimeError(f"worker is not a regular file: {path}")
    with path.open("rb") as stream:
        dos = stream.read(64)
        if len(dos) != 64 or dos[:2] != b"MZ":
            raise RuntimeError("detector worker is not a PE executable")
        stream.seek(struct.unpack_from("<I", dos, 0x3C)[0])
        header = stream.read(6)
    if header[:4] != b"PE\0\0" or struct.unpack_from("<H", header, 4)[0] != MACHINE_X64:
        raise RuntimeError("detector worker is not an x64 PE executable")


def validate_model(path: Path, expected_sha256: str) -> None:
    if not path.is_file() or path.is_symlink() or path.stat().st_size > 64 * 1024 * 1024:
        raise RuntimeError("detector model is not a bounded regular file")
    if sha256(path) != expected_sha256:
        raise RuntimeError("model does not match its reviewed PaddleOCR artifact")


def validate_model_readme(path: Path) -> None:
    if sha256(path) != MODEL_README_SHA256:
        raise RuntimeError("detector model README does not match the reviewed artifact")


def compact_recognizer_model(source: Path, target: Path) -> str:
    model = onnx.load(source, load_external_data=False)
    if len(model.graph.output) != 1:
        raise RuntimeError("recognizer model must expose exactly one probability output")
    probabilities = model.graph.output[0].name
    values = f"{probabilities}_top3_values"
    indices = f"{probabilities}_top3_indices"
    opset = next(
        (entry.version for entry in model.opset_import if entry.domain in ("", "ai.onnx")),
        None,
    )
    if opset is None:
        raise RuntimeError("recognizer model has no default ONNX opset")
    if opset >= 10:
        k_name = f"{probabilities}_top3_k"
        model.graph.initializer.append(
            helper.make_tensor(k_name, TensorProto.INT64, [1], [3])
        )
        node = helper.make_node(
            "TopK", [probabilities, k_name], [values, indices], axis=-1, largest=1, sorted=1
        )
    else:
        node = helper.make_node(
            "TopK", [probabilities], [values, indices], axis=-1, k=3
        )
    model.graph.node.append(node)
    del model.graph.output[:]
    model.graph.output.extend(
        [
            helper.make_tensor_value_info(values, TensorProto.FLOAT, [None, None, 3]),
            helper.make_tensor_value_info(indices, TensorProto.INT64, [None, None, 3]),
        ]
    )
    onnx.checker.check_model(model)
    onnx.save_model(model, target)
    onnx.checker.check_model(onnx.load(target, load_external_data=False))
    return sha256(target)


def recognizer_files(
    root: Path, output: Path
) -> tuple[list[tuple[str, Path]], Path, Path]:
    if not root.is_dir() or root.is_symlink():
        raise RuntimeError("recognizer root is not a regular directory")
    packaged = []
    catalog_entries = []
    sources = []
    for identifier, source_name, model_sha, config_sha, reverse_output in RECOGNIZERS:
        source = root / source_name
        if source.is_dir() and not source.is_symlink():
            model = source / "inference.onnx"
            config = source / "inference.yml"
        else:
            source = root / "recognizers" / identifier
            if not source.is_dir() or source.is_symlink():
                raise RuntimeError(
                    f"recognizer source is not a regular directory: {source}"
                )
            model = source / "model.onnx"
            config = source / "config.yml"
        validate_model(model, model_sha)
        validate_model(config, config_sha)
        relative_root = f"recognizers/{identifier}"
        compact_model = output / f".{identifier}-compact.onnx"
        compact_sha = compact_recognizer_model(model, compact_model)
        packaged.extend(
            [
                (f"{relative_root}/model.onnx", compact_model),
                (f"{relative_root}/config.yml", config),
            ]
        )
        entry = {
            "model": f"{relative_root}/model.onnx",
            "config": f"{relative_root}/config.yml",
        }
        if reverse_output:
            entry["reverseOutput"] = True
        coverage = RECOGNIZER_COVERAGE.get(identifier)
        if coverage:
            entry["coverage"] = coverage
        routing = RECOGNIZER_ROUTING.get(identifier)
        if routing:
            entry["routing"] = routing
        catalog_entries.append(entry)
        sources.append(
            {
                "id": identifier,
                "source": f"https://huggingface.co/PaddlePaddle/{source_name}",
                "files": [
                    {"name": "inference.onnx", "sha256": model_sha},
                    {"name": "inference.yml", "sha256": config_sha},
                    {
                        "name": "model.onnx",
                        "sha256": compact_sha,
                        "transformation": "append sorted TopK(3) outputs to the reviewed probability graph",
                    },
                ],
            }
        )
    catalog = output / ".screen-text-detector-recognizers.json"
    catalog.write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "primary": catalog_entries[0],
                "fallbacks": catalog_entries[1:],
            },
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
        )
        + "\n",
        encoding="utf-8",
    )
    source_inventory = output / ".screen-text-detector-model-sources.json"
    source_inventory.write_text(
        json.dumps(
            {"schemaVersion": 1, "models": sources},
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
        )
        + "\n",
        encoding="utf-8",
    )
    return packaged, catalog, source_inventory


def license_files(package_root: Path) -> list[Path]:
    return sorted(
        path
        for path in package_root.iterdir()
        if path.is_file()
        and not path.is_symlink()
        and path.name.casefold().startswith(LICENSE_PREFIXES)
    )


def dependency_packages(repo: Path, manifest: Path) -> list[dict[str, object]]:
    result = subprocess.run(
        [
            "cargo",
            "metadata",
            "--locked",
            "--filter-platform",
            "x86_64-pc-windows-msvc",
            "--format-version",
            "1",
            "--manifest-path",
            str(manifest),
        ],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    metadata = json.loads(result.stdout)
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    pending = [metadata["resolve"]["root"]]
    reachable: set[str] = set()
    while pending:
        package_id = pending.pop()
        if package_id in reachable:
            continue
        reachable.add(package_id)
        pending.extend(dependency["pkg"] for dependency in nodes[package_id]["deps"])
    return [
        packages[package_id]
        for package_id in reachable
        if packages[package_id]["name"] not in FIRST_PARTY
    ]


def write_rust_licenses(repo: Path, manifest: Path, output: Path) -> tuple[Path, Path]:
    records = []
    notices = [
        "Screen Goated Toolbox - Screen Translate detector third-party notices",
        "Full upstream license files follow for every resolved Rust dependency.",
    ]
    packages = sorted(
        dependency_packages(repo, manifest), key=lambda item: (item["name"], item["version"])
    )
    for package in packages:
        files = license_files(Path(package["manifest_path"]).parent)
        if not files:
            raise RuntimeError(f"license files are missing for {package['name']}")
        file_records = []
        notices.append(
            f"\n{'=' * 78}\n{package['name']} {package['version']}\n"
            f"License: {package.get('license')}\nSource: {package.get('repository')}"
        )
        for path in files:
            data = path.read_bytes()
            file_records.append(
                {
                    "name": path.name,
                    "sizeBytes": len(data),
                    "sha256": hashlib.sha256(data).hexdigest(),
                    "base64": base64.b64encode(data).decode("ascii"),
                }
            )
            notices.append(f"\n--- {path.name} ---\n{data.decode('utf-8').rstrip()}\n")
        records.append(
            {
                "name": package["name"],
                "version": package["version"],
                "licenseExpression": package.get("license"),
                "repository": package.get("repository"),
                "files": file_records,
            }
        )
    inventory = output / ".screen-text-detector-third-party-licenses.json"
    notice = output / ".screen-text-detector-third-party-notices.txt"
    inventory.write_text(
        json.dumps(
            {"schemaVersion": 1, "rust": records},
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
        )
        + "\n",
        encoding="utf-8",
    )
    notice.write_text("\n".join(notices), encoding="utf-8")
    return inventory, notice


def deterministic_zip(target: Path, files: list[tuple[str, Path]]) -> None:
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for relative, source in sorted(files):
            info = zipfile.ZipInfo(relative, FIXED_TIMESTAMP)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.create_system = 3
            info.external_attr = (0o100755 if relative.endswith(".exe") else 0o100644) << 16
            with source.open("rb") as reader, archive.open(info, "w") as writer:
                for chunk in iter(lambda: reader.read(1024 * 1024), b""):
                    writer.write(chunk)


def package(
    repo: Path,
    output: Path,
    worker: Path,
    detector_model: Path,
    detector_onnx: Path,
    recognizer_root: Path,
    model_readme: Path,
    paddle_license: Path,
) -> dict[str, object]:
    manifest = repo / "native/screen_text_detector_worker/Cargo.toml"
    validate_x64_pe(worker)
    validate_model(detector_model, DETECTOR_SHA256)
    validate_model(detector_onnx, DETECTOR_ONNX_SHA256)
    validate_model_readme(model_readme)
    recognizers, recognizer_catalog, model_sources = recognizer_files(
        recognizer_root, output
    )
    inventory, notices = write_rust_licenses(repo, manifest, output)
    model_root = "models/pp-ocr-screen-text"
    files = [
        ("bin/x64/sgt-screen-text-detector-worker.exe", worker),
        (f"{model_root}/detector.onnx", detector_onnx),
        (f"{model_root}/detector.ort", detector_model),
        (f"{model_root}/recognizers.json", recognizer_catalog),
        ("licenses/THIRD-PARTY-LICENSES.json", inventory),
        ("licenses/THIRD-PARTY-NOTICES.txt", notices),
        ("licenses/PaddleOCR-LICENSE.txt", paddle_license),
        ("licenses/PaddleOCR-MODELS.json", model_sources),
        ("licenses/PP-OCRv5-mobile-det-README.md", model_readme),
    ]
    files.extend((f"{model_root}/{relative}", source) for relative, source in recognizers)
    first = output / ".screen-text-detector.first.zip"
    second = output / ".screen-text-detector.second.zip"
    first.unlink(missing_ok=True)
    second.unlink(missing_ok=True)
    deterministic_zip(first, files)
    deterministic_zip(second, files)
    if sha256(first) != sha256(second):
        raise RuntimeError("detector archive is nondeterministic")
    second.unlink()
    version = worker_version(manifest)
    archive_hash = sha256(first)
    asset = f"{COMPONENT_ID}-{version}-{archive_hash[:16]}.zip"
    target = output / asset
    if target.exists() and sha256(target) != archive_hash:
        raise RuntimeError(f"refusing to replace immutable asset: {target}")
    if target.exists():
        first.unlink()
    else:
        first.replace(target)
    records = [
        {"path": relative, "sizeBytes": source.stat().st_size, "sha256": sha256(source)}
        for relative, source in sorted(files)
    ]
    return {
        "id": COMPONENT_ID,
        "version": version,
        "asset": asset,
        "assetPath": manifest_asset_path(repo, target),
        "sizeBytes": target.stat().st_size,
        "sha256": archive_hash,
        "unpackedSizeBytes": sum(record["sizeBytes"] for record in records),
        "files": records,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--worker-exe", required=True)
    parser.add_argument("--detector-model", required=True)
    parser.add_argument("--detector-onnx", required=True)
    parser.add_argument("--recognizer-root", required=True)
    parser.add_argument("--model-readme", required=True)
    parser.add_argument(
        "--paddle-license", default="third_party/egui-wgpu/LICENSE-APACHE"
    )
    parser.add_argument("--output-dir", default="local-runtime-bundles/screen-text-detector")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    output = (repo / args.output_dir).resolve()
    worker = (repo / args.worker_exe).resolve()
    detector_model = Path(args.detector_model).resolve()
    detector_onnx = Path(args.detector_onnx).resolve()
    recognizer_root = Path(args.recognizer_root).resolve()
    model_readme = Path(args.model_readme).resolve()
    paddle_license = (repo / args.paddle_license).resolve()
    for source in (
        worker,
        detector_model,
        detector_onnx,
        model_readme,
        paddle_license,
    ):
        if not source.is_file() or source.is_symlink():
            raise RuntimeError(f"package input is not a regular file: {source}")
    output.mkdir(parents=True, exist_ok=True)
    component = package(
        repo,
        output,
        worker,
        detector_model,
        detector_onnx,
        recognizer_root,
        model_readme,
        paddle_license,
    )
    descriptor = {"schemaVersion": 1, "architecture": "x64", "component": component}
    packages_path = output / "screen_text_detector.packages.json"
    packages_path.write_text(
        json.dumps(descriptor, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(packages_path)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
