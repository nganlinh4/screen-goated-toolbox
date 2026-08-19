import json
import os
import struct
import subprocess
import sys
import time
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab_paths


MAGIC = b"SGTD"
PROTOCOL_VERSION = 3
HEADER = struct.Struct("<4sHHQI")


def component_root(components: Path, component_id: str, required: str) -> Path:
    candidates = []
    for version in (components / component_id).iterdir():
        receipt_path = version / "receipt.json"
        required_path = version / required
        if not version.is_dir() or not receipt_path.is_file() or not required_path.is_file():
            continue
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        if receipt.get("id") == component_id and receipt.get("version") == version.name:
            candidates.append(version)
    if not candidates:
        raise RuntimeError(f"No installed {component_id} component is usable")
    return max(candidates, key=lambda path: path.stat().st_mtime_ns)


def wide_path(path: Path) -> bytes:
    encoded = str(path.resolve()).encode("utf-16-le")
    return struct.pack("<I", len(encoded) // 2) + encoded


def write_frame(pipe, kind: int, request_id: int, payload: bytes) -> None:
    pipe.write(HEADER.pack(MAGIC, PROTOCOL_VERSION, kind, request_id, len(payload)))
    pipe.write(payload)
    pipe.flush()


def read_exact(pipe, size: int) -> bytes:
    value = pipe.read(size)
    if len(value) != size:
        raise RuntimeError("Detector worker closed its output unexpectedly")
    return value


def read_frame(pipe, expected_id: int) -> tuple[int, bytes]:
    magic, version, kind, request_id, length = HEADER.unpack(read_exact(pipe, HEADER.size))
    if magic != MAGIC or version != PROTOCOL_VERSION or request_id != expected_id:
        raise RuntimeError("Detector worker returned an invalid protocol frame")
    return kind, read_exact(pipe, length)


class Cursor:
    def __init__(self, payload: bytes):
        self.payload = payload
        self.offset = 0

    def take(self, size: int) -> bytes:
        end = self.offset + size
        if end > len(self.payload):
            raise RuntimeError("Detector worker returned a truncated payload")
        value = self.payload[self.offset:end]
        self.offset = end
        return value

    def u32(self) -> int:
        return struct.unpack("<I", self.take(4))[0]

    def f32(self) -> float:
        return struct.unpack("<f", self.take(4))[0]

    def text(self) -> str:
        return self.take(self.u32()).decode("utf-8")

    def finish(self) -> None:
        if self.offset != len(self.payload):
            raise RuntimeError("Detector worker returned trailing payload data")


def parse_error(payload: bytes) -> str:
    cursor = Cursor(payload)
    message = cursor.text()
    cursor.finish()
    return message


def parse_regions(payload: bytes) -> tuple[int, int, list[dict]]:
    cursor = Cursor(payload)
    width = cursor.u32()
    height = cursor.u32()
    regions = []
    for _ in range(cursor.u32()):
        left, top, right, bottom = [cursor.u32() for _ in range(4)]
        locator_confidence = cursor.f32()
        primary_text = cursor.text()
        primary_confidence = cursor.f32()
        alternatives = [
            {"text": cursor.text(), "confidence": cursor.f32()}
            for _ in range(cursor.u32())
        ]
        regions.append(
            {
                "boxPx": [left, top, right - left, bottom - top],
                "locatorConfidence": locator_confidence,
                "primaryText": primary_text,
                "primaryConfidence": primary_confidence,
                "alternatives": alternatives,
            }
        )
    cursor.finish()
    return width, height, regions


def save_preview(
    source: Path,
    destination: Path,
    regions: list[dict],
    numbered: bool = False,
) -> None:
    with Image.open(source) as opened:
        image = opened.convert("RGB")
    draw = ImageDraw.Draw(image)
    font_size = max(9, min(13, round(min(image.size) * 0.02)))
    font = ImageFont.load_default()
    for font_path in (
        r"C:\Windows\Fonts\malgunbd.ttf",
        r"C:\Windows\Fonts\YuGothB.ttc",
        r"C:\Windows\Fonts\arial.ttf",
    ):
        try:
            font = ImageFont.truetype(font_path, font_size)
            break
        except OSError:
            continue
    for index, region in enumerate(regions, start=1):
        left, top, width, height = region["boxPx"]
        draw.rectangle(
            (left, top, left + width - 1, top + height - 1),
            outline=(255, 180, 0),
            width=2 if numbered else 1,
        )
        if not numbered:
            continue
        text = " ".join(region["primaryText"].split()) or "∅"
        confidence = round(region["primaryConfidence"] * 100)
        prefix = f"{index} · {confidence}% · "
        maximum_label_width = min(image.width, 220)
        label = prefix + text
        while len(text) > 1:
            bounds = draw.textbbox((0, 0), label, font=font, anchor="lt")
            if bounds[2] - bounds[0] + 6 <= maximum_label_width:
                break
            text = text[:-1]
            label = prefix + text.rstrip() + "…"
        label_box = draw.textbbox((0, 0), label, font=font, anchor="lt")
        label_width = min(image.width, label_box[2] - label_box[0] + 6)
        label_height = label_box[3] - label_box[1] + 4
        label_left = min(left, max(0, image.width - label_width))
        if top >= label_height:
            label_top = top - label_height
        elif top + height + label_height <= image.height:
            label_top = top + height
        else:
            label_top = min(top, image.height - label_height)
        draw.rounded_rectangle(
            (
                label_left,
                label_top,
                label_left + label_width - 1,
                label_top + label_height - 1,
            ),
            radius=2,
            fill=(22, 22, 22),
            outline=(255, 180, 0),
        )
        draw.text(
            (label_left + 3, label_top + 2),
            label,
            fill=(255, 244, 205),
            font=font,
            anchor="lt",
        )
    image.save(destination, format="JPEG", quality=88)


def save_viewer_data(root: Path) -> None:
    cases = []
    for case in sorted((root / "inputs").iterdir()):
        source = case / "source.jpg"
        raw = case / "detector-raw.json"
        run = case / "detector-run.json"
        if not source.is_file() or not raw.is_file():
            continue
        with Image.open(source) as image:
            size = list(image.size)
        cases.append(
            {
                "name": case.name,
                "source": f"inputs/{case.name}/source.jpg",
                "size": size,
                "regions": json.loads(raw.read_text(encoding="utf-8")),
                "run": json.loads(run.read_text(encoding="utf-8")) if run.is_file() else {},
            }
        )
    payload = json.dumps(cases, ensure_ascii=False, separators=(",", ":"))
    operation = json.dumps(
        {
            "mode": "Raw worker output",
            "locatorPixelThreshold": 0.15,
            "locatorBoxThreshold": 0.8,
            "hostOcrThreshold": 0.5,
            "hostOcrThresholdApplied": False,
            "inferenceLongSide": 1600,
        },
        separators=(",", ":"),
    )
    (root / "viewer-data.js").write_text(
        f"window.DETECTOR_OPERATION={operation};\nwindow.DETECTOR_CASES={payload};\n",
        encoding="utf-8",
    )


def main() -> int:
    root = lab_paths.artifact_root()
    inputs = lab_paths.inputs_root()
    cases = sorted(path for path in inputs.iterdir() if path.is_dir() and (path / "source.jpg").is_file())
    if not cases:
        print(f"No case folders contain source.jpg under {inputs}")
        return 0

    local_app_data = Path(os.environ["LOCALAPPDATA"])
    components = local_app_data / "screen-goated-toolbox" / "components"
    detector = component_root(
        components,
        "screen-text-detector",
        "bin/x64/sgt-screen-text-detector-worker.exe",
    )
    runtime = component_root(components, "onnx-directml-runtime", "bin/x64/onnxruntime.dll")
    vc_runtime = component_root(components, "vc14-x64-runtime", "bin/x64/vcruntime140.dll")
    worker = detector / "bin/x64/sgt-screen-text-detector-worker.exe"
    model_dir = detector / "models/pp-ocr-screen-text"
    runtime_bin = runtime / "bin/x64"
    vc_bin = vc_runtime / "bin/x64"
    system_root = Path(os.environ["SystemRoot"])
    environment = {
        "SystemRoot": str(system_root),
        "WINDIR": str(system_root),
        "PATH": os.pathsep.join((str(runtime_bin), str(vc_bin), str(system_root / "System32"))),
        "TEMP": os.environ["TEMP"],
        "TMP": os.environ["TMP"],
    }
    workspace = root / "worker"
    workspace.mkdir(exist_ok=True)
    started = time.perf_counter()
    process = subprocess.Popen(
        [str(worker), "--stdio"],
        cwd=workspace,
        env=environment,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        creationflags=subprocess.CREATE_NO_WINDOW,
    )
    try:
        nonce = os.urandom(32)
        hello = nonce + wide_path(runtime_bin) + wide_path(model_dir)
        write_frame(process.stdin, 1, 1, hello)
        kind, payload = read_frame(process.stdout, 1)
        if kind == 199:
            raise RuntimeError(parse_error(payload))
        cursor = Cursor(payload)
        echoed_nonce = cursor.take(32)
        worker_version = cursor.text()
        cursor.finish()
        if kind != 101 or echoed_nonce != nonce:
            raise RuntimeError("Detector worker handshake identity mismatch")
        initialization_ms = (time.perf_counter() - started) * 1000

        for request_id, case in enumerate(cases, start=2):
            source = case / "source.jpg"
            jpeg = source.read_bytes()
            detected_at = time.perf_counter()
            write_frame(process.stdin, 2, request_id, jpeg)
            kind, payload = read_frame(process.stdout, request_id)
            if kind == 199:
                raise RuntimeError(f"{case.name}: {parse_error(payload)}")
            if kind != 102:
                raise RuntimeError(f"{case.name}: unexpected detector response {kind}")
            width, height, regions = parse_regions(payload)
            detection_ms = (time.perf_counter() - detected_at) * 1000
            (case / "detector-raw.json").write_text(
                json.dumps(regions, ensure_ascii=False, indent=2),
                encoding="utf-8",
            )
            save_preview(source, case / "detector-raw.jpg", regions)
            save_preview(
                source,
                case / "detector-raw-numbered.jpg",
                regions,
                numbered=True,
            )
            (case / "detector-run.json").write_text(
                json.dumps(
                    {
                        "workerVersion": worker_version,
                        "imageSize": [width, height],
                        "regionCount": len(regions),
                        "initializationMs": round(initialization_ms, 2),
                        "detectionMs": round(detection_ms, 2),
                    },
                    indent=2,
                ),
                encoding="utf-8",
            )
            print(f"{case.name}: {len(regions)} regions in {detection_ms:.1f} ms")

        save_viewer_data(root)
        write_frame(process.stdin, 3, len(cases) + 2, b"")
        read_frame(process.stdout, len(cases) + 2)
    finally:
        if process.poll() is None:
            process.kill()
        _, stderr = process.communicate(timeout=5)
        if process.returncode not in (0, 1) and stderr:
            print(stderr.decode("utf-8", errors="replace"), file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
