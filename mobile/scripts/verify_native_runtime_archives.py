#!/usr/bin/env python3
"""Verify Android native runtime archives against their delivery contracts."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import zipfile


HEX_256 = re.compile(r"[0-9a-f]{64}")
RUNTIME_PREFIX = (
    "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/"
    "sgt-runtime-bundles/"
)


def load_json(path: Path) -> dict:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"Expected an object in {path}")
    return value


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def require_keys(value: dict, expected: set[str], label: str) -> None:
    if set(value) != expected:
        raise ValueError(f"{label} has unsupported fields: {sorted(set(value) - expected)}")


def elf_section_names(data: bytes) -> set[str]:
    if len(data) < 64 or data[:4] != b"\x7fELF":
        raise ValueError("Sherpa runtime is not ELF")
    if data[4:6] != b"\x02\x01" or struct.unpack_from("<H", data, 18)[0] != 183:
        raise ValueError("Sherpa runtime must be little-endian AArch64 ELF64")
    section_offset = struct.unpack_from("<Q", data, 40)[0]
    entry_size, count, string_index = struct.unpack_from("<HHH", data, 58)
    if entry_size < 64 or string_index >= count:
        raise ValueError("Sherpa ELF section table is invalid")

    def section(index: int) -> int:
        offset = section_offset + index * entry_size
        if offset + entry_size > len(data):
            raise ValueError("Sherpa ELF section lies outside the file")
        return offset

    string_header = section(string_index)
    string_offset, string_size = struct.unpack_from("<QQ", data, string_header + 24)
    if string_offset + string_size > len(data):
        raise ValueError("Sherpa ELF string table lies outside the file")
    names: set[str] = set()
    for index in range(count):
        name_offset = struct.unpack_from("<I", data, section(index))[0]
        if name_offset >= string_size:
            raise ValueError("Sherpa ELF section name lies outside the string table")
        start = string_offset + name_offset
        end = data.find(b"\0", start, string_offset + string_size)
        if end < 0:
            end = string_offset + string_size
        names.add(data[start:end].decode("ascii"))
    return names


def verify_sherpa_sources(mobile_root: Path, spec_dir: Path, build: dict) -> tuple[dict, dict]:
    if build.get("schemaVersion") != 1 or build.get("abi") != "arm64-v8a":
        raise ValueError("Sherpa build contract identity is invalid")
    artifact = build["artifact"]
    java_use = build["javaUse"]
    elf = build["elf"]
    generation = build["operatorGeneration"]
    source_patch = build["sourcePatch"]
    checks = (
        (spec_dir / generation["configFile"], generation["configSha256"]),
        (spec_dir / generation["modelsFile"], generation["modelsSha256"]),
        (spec_dir / source_patch["file"], source_patch["sha256"]),
    )
    for path, expected in checks:
        if sha256(path) != expected:
            raise ValueError(f"Sherpa source input differs from its build contract: {path}")

    notices = spec_dir / "assets/third_party/sherpa-runtime"
    actual_notices = {path.name for path in notices.iterdir() if path.is_file() and path.stat().st_size > 0}
    if actual_notices != set(build["noticeFiles"]):
        raise ValueError("Sherpa notice files differ from their build contract")

    java_text = (mobile_root / java_use["source"]).read_text(encoding="utf-8")
    actual_types = set(re.findall(r"com\.k2fsa\.sherpa\.onnx\.([A-Za-z0-9_]+)", java_text))
    if actual_types != set(java_use["types"]):
        raise ValueError("Sherpa Java type use differs from its build contract")
    for receiver, key in (("recognizer", "recognizerMethods"), ("stream", "streamMethods")):
        methods = set(re.findall(rf"\b{receiver}\.([A-Za-z0-9_]+)\s*\(", java_text))
        if methods != set(java_use[key]):
            raise ValueError(f"Sherpa {receiver} method use differs from its build contract")
    return artifact, elf


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mobile-root", required=True, type=Path)
    parser.add_argument("--contract", required=True, type=Path)
    parser.add_argument("--archive-dir", required=True, type=Path)
    parser.add_argument("--sherpa-spec-dir", required=True, type=Path)
    parser.add_argument("--sherpa-build-contract", required=True, type=Path)
    args = parser.parse_args()

    contract = load_json(args.contract)
    require_keys(contract, {"schemaVersion", "abi", "archives"}, "Native runtime contract")
    if contract["schemaVersion"] != 1 or contract["abi"] != "arm64-v8a":
        raise ValueError("Native runtime contract identity is invalid")
    sherpa_build = load_json(args.sherpa_build_contract)
    if sherpa_build.get("abi") != contract["abi"]:
        raise ValueError("Sherpa and delivery ABI contracts differ")
    sherpa_artifact, sherpa_elf = verify_sherpa_sources(
        args.mobile_root, args.sherpa_spec_dir, sherpa_build
    )

    expected_names: set[str] = set()
    expected_engines: set[str] = set()
    for archive in contract["archives"]:
        require_keys(
            archive,
            {"engine", "fileName", "downloadUrl", "byteCount", "sha256", "fullDelivery", "entries"},
            "Native runtime archive",
        )
        engine = archive["engine"]
        if engine in expected_engines:
            raise ValueError(f"Duplicate native runtime engine: {engine}")
        expected_engines.add(engine)
        if archive["fullDelivery"] != "verified_download":
            raise ValueError(f"Full native runtime delivery is invalid for {engine}")
        name = archive["fileName"]
        if Path(name).name != name or not name.endswith("-runtime.zip") or name in expected_names:
            raise ValueError(f"Native runtime archive name is invalid: {name}")
        expected_names.add(name)
        url = archive["downloadUrl"]
        asset = url.removeprefix(RUNTIME_PREFIX)
        if not url.startswith(RUNTIME_PREFIX) or not asset or "/" in asset or not asset.endswith(".zip"):
            raise ValueError(f"Native runtime URL is invalid: {url}")
        path = args.archive_dir / name
        if (
            not path.is_file()
            or path.stat().st_size != archive["byteCount"]
            or not HEX_256.fullmatch(archive["sha256"])
            or sha256(path) != archive["sha256"]
        ):
            raise ValueError(f"Native runtime archive identity differs: {name}")

        entries = archive["entries"]
        expected_entries = {entry["fileName"]: entry for entry in entries}
        if len(expected_entries) != len(entries):
            raise ValueError(f"{name} contains duplicate member contracts")
        for entry_name, entry in expected_entries.items():
            require_keys(entry, {"fileName", "byteCount", "sha256"}, f"{name}/{entry_name}")
            if (
                Path(entry_name).name != entry_name
                or not entry_name.endswith(".so")
                or entry["byteCount"] <= 0
                or not HEX_256.fullmatch(entry["sha256"])
            ):
                raise ValueError(f"Native runtime member contract is invalid: {name}/{entry_name}")

        with zipfile.ZipFile(path) as runtime_zip:
            infos = runtime_zip.infolist()
            names = [info.filename for info in infos]
            if any(info.is_dir() for info in infos) or len(names) != len(set(names)):
                raise ValueError(f"{name} contains directories or duplicate entries")
            if set(names) != set(expected_entries):
                raise ValueError(f"{name} members differ from its contract")
            for info in infos:
                expected = expected_entries[info.filename]
                data = runtime_zip.read(info)
                if info.file_size != expected["byteCount"] or hashlib.sha256(data).hexdigest() != expected["sha256"]:
                    raise ValueError(f"Native runtime member identity differs: {name}/{info.filename}")
            if engine == "sherpa":
                if name != "sherpa-runtime.zip" or sherpa_artifact["fileName"] != "libsherpa-onnx-jni.so":
                    raise ValueError("Sherpa runtime artifact naming differs")
                native = runtime_zip.read(sherpa_artifact["fileName"])
                if (
                    len(native) != sherpa_artifact["byteCount"]
                    or hashlib.sha256(native).hexdigest() != sherpa_artifact["sha256"]
                    or len(native) > sherpa_elf["maximumByteCount"]
                ):
                    raise ValueError("Sherpa ELF identity or size ceiling differs")
                text = native.decode("latin1")
                exports = set(re.findall(r"Java_com_k2fsa_sherpa_onnx_[A-Za-z0-9_]+", text))
                if exports != set(sherpa_build["jniExports"]):
                    raise ValueError("Sherpa JNI exports differ from its build contract")
                if not all(value in text for value in sherpa_elf["needed"]):
                    raise ValueError("Sherpa ELF is missing a required Android dependency")
                if any(value in text for value in sherpa_elf["forbiddenNeeded"]):
                    raise ValueError("Sherpa ELF gained a forbidden shared runtime dependency")
                sections = elf_section_names(native)
                if any(name.startswith(".debug") for name in sections) or ".symtab" in sections:
                    raise ValueError("Sherpa ELF retains debug or static symbol sections")

    if expected_engines != {"ort", "moonshine", "sherpa"}:
        raise ValueError(f"Native runtime engine set differs: {sorted(expected_engines)}")
    actual_names = {path.name for path in args.archive_dir.glob("*-runtime.zip") if path.is_file()}
    if actual_names != expected_names:
        raise ValueError("Checked-in native runtime archives differ from their contract")


if __name__ == "__main__":
    main()
