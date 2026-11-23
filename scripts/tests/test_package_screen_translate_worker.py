from __future__ import annotations

import importlib.util
import json
import re
import struct
import sys
import tempfile
import unittest
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "package_screen_translate_worker",
    REPO / "scripts" / "package_screen_translate_worker.py",
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def pe_header(machine: int) -> bytes:
    dos = bytearray(64)
    dos[:2] = b"MZ"
    struct.pack_into("<I", dos, 0x3C, 64)
    return bytes(dos) + b"PE\0\0" + struct.pack("<H", machine)


class RetiredDetectorPackagingTests(unittest.TestCase):
    def test_the_retired_detector_packager_is_gone_and_nothing_imports_it(self) -> None:
        self.assertFalse((REPO / "scripts/package_screen_text_detector.py").exists())
        self.assertFalse((REPO / "native/screen_text_detector_worker").exists())
        for script in (REPO / "scripts").glob("*.py"):
            source = script.read_text(encoding="utf-8")
            self.assertNotRegex(source, r"\bpackage_screen_text_detector\b", script.name)

    def test_the_descriptor_declares_the_streaming_protocol_the_host_accepts(self) -> None:
        protocol = (REPO / "src/component_registry/screen_text_detector/protocol.rs").read_text(
            encoding="utf-8"
        )
        self.assertIn(f'const STREAM: &str = "{MODULE.PROTOCOL}";', protocol)
        self.assertNotEqual(MODULE.PROTOCOL, "sgtd-v4")

    def test_the_packaged_version_is_the_version_the_worker_reports(self) -> None:
        version = MODULE.worker_version(REPO / "native/screen_translate_worker/Cargo.toml")
        stream = (REPO / "native/screen_text_detector_protocol/src/stream.rs").read_text(
            encoding="utf-8"
        )
        reported = re.search(r'pub const WORKER_VERSION: &str = "([^"]+)";', stream)
        self.assertIsNotNone(reported)
        self.assertEqual(version, reported.group(1))


class ScreenTranslatePackagingTests(unittest.TestCase):
    def test_notices_cover_reachable_third_party_crates_only(self) -> None:
        def package(name: str) -> dict:
            return {"id": name, "name": name}

        metadata = {
            "packages": [
                package("sgt-screen-translate-worker"),
                package("sgt-screen-text-detector-protocol"),
                package("serde"),
                package("ort"),
                package("unreachable"),
            ],
            "resolve": {
                "root": "sgt-screen-translate-worker",
                "nodes": [
                    {
                        "id": "sgt-screen-translate-worker",
                        "deps": [{"pkg": "sgt-screen-text-detector-protocol"}, {"pkg": "ort"}],
                    },
                    {"id": "sgt-screen-text-detector-protocol", "deps": [{"pkg": "serde"}]},
                    {"id": "serde", "deps": []},
                    {"id": "ort", "deps": []},
                    {"id": "unreachable", "deps": []},
                ],
            },
        }
        names = sorted(item["name"] for item in MODULE.third_party_packages(metadata))
        self.assertEqual(names, ["ort", "serde"])

    def test_worker_must_be_an_x64_pe(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            worker = Path(temporary) / "worker.exe"
            worker.write_bytes(pe_header(MODULE.MACHINE_X64))
            MODULE.validate_x64_pe(worker)
            worker.write_bytes(pe_header(0x014C))
            with self.assertRaisesRegex(RuntimeError, "x64"):
                MODULE.validate_x64_pe(worker)

    def test_model_inputs_must_be_relative_and_match_their_pins(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "models").mkdir()
            model = root / "models/detector.onnx"
            model.write_bytes(b"model")
            digest = MODULE.sha256(model)
            inputs = {"models": [{"path": "models/detector.onnx", "sha256": digest}], "licenses": []}
            self.assertEqual(
                MODULE.model_inputs(inputs, root, root), [("models/detector.onnx", model)]
            )
            inputs["models"][0]["sha256"] = "0" * 64
            with self.assertRaisesRegex(ValueError, "pinned bytes"):
                MODULE.model_inputs(inputs, root, root)
            inputs["models"][0] = {"path": "../escape.onnx", "sha256": digest}
            with self.assertRaisesRegex(ValueError, "relative"):
                MODULE.model_inputs(inputs, root, root)

    def test_runtime_inventory_lists_every_tracked_package_input(self) -> None:
        inputs = json.loads(
            (REPO / "native/screen_translate_worker/package-inputs.json").read_text(
                encoding="utf-8"
            )
        )
        contract = json.loads(
            (REPO / "component-delivery/windows/screen-text-detector-v1.json").read_text(
                encoding="utf-8"
            )
        )
        shipped = {item["path"] for item in contract["component"]["files"]}
        expected = {item["path"] for section in ("models", "licenses") for item in inputs[section]}
        expected |= {
            MODULE.WORKER_PATH,
            "licenses/THIRD-PARTY-LICENSES.json",
            "licenses/THIRD-PARTY-NOTICES.txt",
            "licenses/PACKAGE-INPUTS.json",
            "models/pp-ocr-screen-text/readers.json",
        }
        self.assertEqual(shipped, expected)


if __name__ == "__main__":
    unittest.main()
