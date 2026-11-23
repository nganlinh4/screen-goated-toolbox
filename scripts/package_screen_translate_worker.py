#!/usr/bin/env python3
"""Package the pinned native incremental Screen Translate component."""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import shutil
import struct
import subprocess
import zipfile
from pathlib import Path

FIXED_TIMESTAMP = (1980, 1, 1, 0, 0, 0)
MACHINE_X64 = 0x8664
COMPONENT_ID = "screen-text-detector"
PROTOCOL = "sgts-v1"
WORKER_PATH = "bin/x64/sgt-screen-text-detector-worker.exe"
LICENSE_PREFIXES = ("license", "licence", "copying", "notice", "copyright")
# First-party crates carry the repository license, not a third-party notice.
FIRST_PARTY = frozenset({"sgt-screen-translate-worker", "sgt-screen-text-detector-protocol"})


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def worker_version(manifest: Path) -> str:
    match = re.search(
        r'^version\s*=\s*"([^"]+)"', manifest.read_text(encoding="utf-8"), re.MULTILINE
    )
    if not match:
        raise RuntimeError("Screen Translate worker version is missing")
    return match.group(1)


def validate_x64_pe(path: Path) -> None:
    if not path.is_file() or path.is_symlink():
        raise RuntimeError(f"worker is not a regular file: {path}")
    with path.open("rb") as stream:
        dos = stream.read(64)
        if len(dos) != 64 or dos[:2] != b"MZ":
            raise RuntimeError("Screen Translate worker is not a PE executable")
        stream.seek(struct.unpack_from("<I", dos, 0x3C)[0])
        header = stream.read(6)
    if header[:4] != b"PE\0\0" or struct.unpack_from("<H", header, 4)[0] != MACHINE_X64:
        raise RuntimeError("Screen Translate worker is not an x64 PE executable")


def verify(path: Path, digest: str) -> Path:
    if not path.is_file() or path.is_symlink() or sha256(path) != digest:
        raise ValueError(f"Package input differs from pinned bytes: {path}")
    return path


def compact_recognizer_model(source: Path, target: Path) -> str:
    """Append a top-3 output so the worker never copies the full probability grid."""
    import onnx
    from onnx import TensorProto, helper

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
        model.graph.initializer.append(helper.make_tensor(k_name, TensorProto.INT64, [1], [3]))
        node = helper.make_node(
            "TopK", [probabilities, k_name], [values, indices], axis=-1, largest=1, sorted=1
        )
    else:
        node = helper.make_node("TopK", [probabilities], [values, indices], axis=-1, k=3)
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


def license_files(package_root: Path) -> list[Path]:
    return sorted(
        path
        for path in package_root.iterdir()
        if path.is_file()
        and not path.is_symlink()
        and path.name.casefold().startswith(LICENSE_PREFIXES)
    )


def third_party_packages(metadata: dict) -> list[dict[str, object]]:
    """Every package reachable from the resolve root, minus first-party crates."""
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


def dependency_packages(repo: Path, manifest: Path) -> list[dict[str, object]]:
    result = subprocess.run(
        [
            "cargo", "metadata", "--locked", "--filter-platform", "x86_64-pc-windows-msvc",
            "--format-version", "1", "--manifest-path", str(manifest),
        ],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    return third_party_packages(json.loads(result.stdout))


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


def archive(path: Path, files: list[tuple[str, Path]]) -> None:
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=1) as target:
        for relative, source in sorted(files):
            info = zipfile.ZipInfo(relative, FIXED_TIMESTAMP)
            info.compress_type = (
                zipfile.ZIP_STORED if relative.endswith(".gguf") else zipfile.ZIP_DEFLATED
            )
            info._compresslevel = 1
            info.create_system = 3
            info.external_attr = (0o100755 if relative.endswith(".exe") else 0o100644) << 16
            with source.open("rb") as reader, target.open(info, "w") as writer:
                shutil.copyfileobj(reader, writer, 1024 * 1024)


def model_inputs(inputs: dict, input_root: Path, output: Path) -> list[tuple[str, Path]]:
    files = []
    for section in ["models", "licenses"]:
        for record in inputs[section]:
            relative = Path(record["path"])
            if relative.is_absolute() or ".." in relative.parts:
                raise ValueError("Package input must be relative")
            source = verify(input_root / relative, record["sha256"])
            if transformation := record.get("transformation"):
                if transformation != "sorted-top3":
                    raise ValueError("Unknown model transformation")
                transformed = output / (record["packagedSha256"] + ".onnx")
                if compact_recognizer_model(source, transformed) != record["packagedSha256"]:
                    raise ValueError("Transformed model differs from reviewed bytes")
                source = transformed
            files.append((record["path"], source))
    return files


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["worker", "input-root", "output-dir"]:
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    output = args.output_dir.resolve()
    if output.is_relative_to(repo):
        raise ValueError("Package output must be outside the repository")
    output.mkdir(parents=True, exist_ok=True)
    manifest = repo / "native/screen_translate_worker/Cargo.toml"
    inputs_path = manifest.with_name("package-inputs.json")
    inputs = json.loads(inputs_path.read_text(encoding="utf-8"))
    version = worker_version(manifest)
    validate_x64_pe(args.worker)
    files = [(WORKER_PATH, args.worker)]
    files.extend(model_inputs(inputs, args.input_root, output))
    inventory, notices = write_rust_licenses(repo, manifest, output)
    files.extend(
        [
            ("licenses/THIRD-PARTY-LICENSES.json", inventory),
            ("licenses/THIRD-PARTY-NOTICES.txt", notices),
            ("licenses/PACKAGE-INPUTS.json", inputs_path),
            ("models/pp-ocr-screen-text/readers.json", manifest.with_name("readers.json")),
        ]
    )
    records = [
        {"path": relative, "sizeBytes": path.stat().st_size, "sha256": sha256(path)}
        for relative, path in sorted(files)
    ]
    first = output / "screen-text-detector.candidate.zip"
    second = output / "screen-text-detector.verify.zip"
    archive(first, files)
    digest = sha256(first)
    archive(second, files)
    if sha256(second) != digest:
        raise ValueError("Component package is not deterministic")
    second.unlink()
    name = f"{COMPONENT_ID}-{version}-{digest[:16]}.zip"
    destination = output / name
    if destination.exists():
        verify(destination, digest)
        first.unlink()
    else:
        first.rename(destination)
    component = {
        "id": COMPONENT_ID,
        "version": version,
        "protocol": PROTOCOL,
        "asset": name,
        "assetPath": str(destination),
        "sizeBytes": destination.stat().st_size,
        "sha256": digest,
        "unpackedSizeBytes": sum(record["sizeBytes"] for record in records),
        "files": records,
    }
    if component["sizeBytes"] > 2_000_000_000:
        raise ValueError("Component exceeds the release asset boundary")
    descriptor = output / "screen_text_detector.packages.json"
    descriptor.write_text(
        json.dumps({"schemaVersion": 1, "architecture": "x64", "component": component}, indent=2)
        + "\n",
        encoding="utf-8",
    )
    print(descriptor)


if __name__ == "__main__":
    main()
