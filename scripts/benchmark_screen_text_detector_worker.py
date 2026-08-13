#!/usr/bin/env python3
"""Validate SGT's native PaddleOCR detector worker against localization fixtures."""

from __future__ import annotations

import argparse
import io
import json
import os
import struct
import subprocess
import time
from pathlib import Path

from PIL import Image, ImageDraw

MAGIC = b"SGTD"
VERSION = 3
HELLO = 1
DETECT = 2
SHUTDOWN = 3
READY = 101
REGIONS = 102
ACK = 103


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--worker", type=Path, required=True)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--cases",
        help="Comma-separated localization case ids; omission selects every case",
    )
    parser.add_argument("--repeat", type=int, default=1)
    return parser.parse_args()


def wide_payload(path: Path) -> bytes:
    encoded = str(path.resolve()).encode("utf-16-le")
    return struct.pack("<I", len(encoded) // 2) + encoded


def write_frame(stream: io.BufferedWriter, kind: int, request_id: int, payload: bytes) -> None:
    stream.write(struct.pack("<4sHHQI", MAGIC, VERSION, kind, request_id, len(payload)))
    stream.write(payload)
    stream.flush()


def read_exact(stream: io.BufferedReader, length: int) -> bytes:
    value = stream.read(length)
    if value is None or len(value) != length:
        raise RuntimeError("detector worker closed its protocol stream")
    return value


def read_frame(stream: io.BufferedReader) -> tuple[int, int, bytes]:
    magic, version, kind, request_id, length = struct.unpack(
        "<4sHHQI", read_exact(stream, 20)
    )
    if magic != MAGIC or version != VERSION or request_id == 0:
        raise RuntimeError("detector worker returned an invalid frame")
    return kind, request_id, read_exact(stream, length)


def jpeg_bytes(image: Image.Image) -> bytes:
    output = io.BytesIO()
    image.convert("RGB").save(output, format="JPEG", quality=88)
    return output.getvalue()


def parse_regions(payload: bytes) -> tuple[int, int, list[dict]]:
    if len(payload) < 12:
        raise RuntimeError("detector response is truncated")
    width, height, count = struct.unpack_from("<III", payload)
    regions = []
    offset = 12
    for _ in range(count):
        left, top, right, bottom, confidence = struct.unpack_from(
            "<IIIIf", payload, offset
        )
        offset += 20
        text_length = struct.unpack_from("<I", payload, offset)[0]
        offset += 4
        text = payload[offset : offset + text_length].decode("utf-8")
        offset += text_length
        text_confidence = struct.unpack_from("<f", payload, offset)[0]
        offset += 4
        alternative_count = struct.unpack_from("<I", payload, offset)[0]
        offset += 4
        alternatives = []
        for _ in range(alternative_count):
            alternative_length = struct.unpack_from("<I", payload, offset)[0]
            offset += 4
            alternative_text = payload[
                offset : offset + alternative_length
            ].decode("utf-8")
            offset += alternative_length
            alternative_confidence = struct.unpack_from("<f", payload, offset)[0]
            offset += 4
            alternatives.append(
                {
                    "text": alternative_text,
                    "confidence": alternative_confidence,
                }
            )
        regions.append(
            {
                "box_px": [left, top, right - left, bottom - top],
                "confidence": confidence,
                "text": text,
                "text_confidence": text_confidence,
                "alternatives": alternatives,
            }
        )
    if offset != len(payload):
        raise RuntimeError("detector region count does not match its payload")
    return width, height, regions


def area(box: list[int]) -> int:
    return box[2] * box[3]


def intersection(left: list[int], right: list[int]) -> int:
    x1, y1 = max(left[0], right[0]), max(left[1], right[1])
    x2 = min(left[0] + left[2], right[0] + right[2])
    y2 = min(left[1] + left[3], right[1] + right[3])
    return max(0, x2 - x1) * max(0, y2 - y1)


def iou(left: list[int], right: list[int]) -> float:
    overlap = intersection(left, right)
    return overlap / max(1, area(left) + area(right) - overlap)


def coverage(gold: list[int], prediction: list[int]) -> float:
    return intersection(gold, prediction) / max(1, area(gold))


def evaluate(case: dict, predictions: list[dict]) -> dict:
    candidates = []
    for gold_index, gold in enumerate(case["regions"]):
        for prediction_index, prediction in enumerate(predictions):
            overlap = iou(gold["box_px"], prediction["box_px"])
            covered = coverage(gold["box_px"], prediction["box_px"])
            if overlap >= 0.15 or covered >= 0.5:
                candidates.append(
                    (0.7 * overlap + 0.3 * covered, gold_index, prediction_index)
                )
    candidates.sort(reverse=True)
    used_gold: set[int] = set()
    used_predictions: set[int] = set()
    matches = []
    for _, gold_index, prediction_index in candidates:
        if gold_index in used_gold or prediction_index in used_predictions:
            continue
        used_gold.add(gold_index)
        used_predictions.add(prediction_index)
        gold = case["regions"][gold_index]["box_px"]
        prediction = predictions[prediction_index]["box_px"]
        matches.append(
            {
                "gold_index": gold_index,
                "prediction_index": prediction_index,
                "iou": iou(gold, prediction),
                "coverage": coverage(gold, prediction),
            }
        )

    def mean(field: str) -> float:
        return sum(match[field] for match in matches) / max(1, len(matches))

    return {
        "expected_regions": len(case["regions"]),
        "predicted_regions": len(predictions),
        "matched_regions": len(matches),
        "region_recall": len(matches) / max(1, len(case["regions"])),
        "mean_iou": mean("iou"),
        "mean_coverage": mean("coverage"),
        "matches": matches,
    }


def draw_overlay(
    image: Image.Image, gold: list[list[int]], predictions: list[list[int]], output: Path
) -> None:
    rendered = image.convert("RGBA")
    painter = ImageDraw.Draw(rendered)
    thickness = max(2, min(5, max(rendered.size) // 700))
    for box, color in [(box, "#00ff70") for box in gold] + [
        (box, "#00d2ff") for box in predictions
    ]:
        x, y, width, height = box
        painter.rectangle(
            [x, y, x + width - 1, y + height - 1], outline=color, width=thickness
        )
    rendered.save(output)


def main() -> None:
    args = parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    creation_flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    process = subprocess.Popen(
        [str(args.worker.resolve()), "--stdio"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        creationflags=creation_flags,
    )
    if process.stdin is None or process.stdout is None:
        raise RuntimeError("detector worker pipes are unavailable")
    nonce = os.urandom(32)
    hello = nonce + wide_payload(args.runtime_dir) + wide_payload(args.model_dir)
    started = time.perf_counter()
    write_frame(process.stdin, HELLO, 1, hello)
    kind, request_id, payload = read_frame(process.stdout)
    initialization_ms = round((time.perf_counter() - started) * 1000, 2)
    if kind != READY or request_id != 1 or payload[:32] != nonce:
        raise RuntimeError("detector worker handshake failed")

    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    selected_ids = {
        value.strip() for value in (args.cases or "").split(",") if value.strip()
    }
    cases = [
        case
        for case in manifest["localization_cases"]
        if not selected_ids or case["id"] in selected_ids
    ]
    if not cases:
        raise RuntimeError("detector benchmark selected no localization cases")
    if selected_ids != {case["id"] for case in cases} and selected_ids:
        raise RuntimeError("detector benchmark contains an unknown case id")
    if args.repeat < 1:
        raise RuntimeError("detector benchmark repeat count must be positive")
    image_root = args.manifest.parent
    reports = []
    requests = [
        (case, iteration)
        for iteration in range(1, args.repeat + 1)
        for case in cases
    ]
    for request_id, (case, iteration) in enumerate(requests, start=2):
        image = Image.open(image_root / case["image"])
        started = time.perf_counter()
        encoded = jpeg_bytes(image)
        encode_ms = round((time.perf_counter() - started) * 1000, 2)
        worker_started = time.perf_counter()
        write_frame(process.stdin, DETECT, request_id, encoded)
        kind, response_id, payload = read_frame(process.stdout)
        worker_ms = round((time.perf_counter() - worker_started) * 1000, 2)
        latency_ms = round((time.perf_counter() - started) * 1000, 2)
        if kind != REGIONS or response_id != request_id:
            raise RuntimeError("detector worker returned an unexpected response")
        width, height, predictions = parse_regions(payload)
        if (width, height) != image.size:
            raise RuntimeError("detector worker changed the image coordinate space")
        metrics = evaluate(case, predictions)
        suffix = f"-run-{iteration}" if args.repeat > 1 else ""
        overlay = args.output / f"{case['id']}{suffix}-worker.png"
        draw_overlay(
            image,
            [region["box_px"] for region in case["regions"]],
            [region["box_px"] for region in predictions],
            overlay,
        )
        reports.append(
            {
                "case_id": case["id"],
                "difficulty": case["difficulty"],
                "iteration": iteration,
                "encode_ms": encode_ms,
                "worker_ms": worker_ms,
                "latency_ms": latency_ms,
                "predictions": predictions,
                "metrics": metrics,
                "overlay": overlay.name,
            }
        )

    write_frame(process.stdin, SHUTDOWN, len(reports) + 2, b"")
    kind, _, _ = read_frame(process.stdout)
    if kind != ACK or process.wait(timeout=5) != 0:
        raise RuntimeError("detector worker did not shut down cleanly")
    summary = {
        "engine": "SGT PaddleOCR detector worker",
        "initialization_ms": initialization_ms,
        "cases": reports,
    }
    (args.output / "summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(json.dumps(summary, ensure_ascii=True))


if __name__ == "__main__":
    main()
