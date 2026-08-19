import json
import math
import os
import threading
import time
from collections import OrderedDict
from pathlib import Path

import cv2
import numpy as np
import onnxruntime as ort
import yaml
from PIL import Image


INFERENCE_LONG_SIDE = 1600
MIN_SIDE = 3
UNCLIP_RATIO = 1.5
MAX_REGIONS = 2000
MAX_CACHED_CASES = 2


def installed_component(components: Path, component_id: str, required: str) -> Path:
    candidates = []
    for version in (components / component_id).iterdir():
        receipt_path = version / "receipt.json"
        if not receipt_path.is_file() or not (version / required).is_file():
            continue
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        if receipt.get("id") == component_id and receipt.get("version") == version.name:
            candidates.append(version)
    if not candidates:
        raise RuntimeError(f"No installed {component_id} component is usable")
    return max(candidates, key=lambda path: path.stat().st_mtime_ns)


def inference_size(width: int, height: int) -> tuple[int, int]:
    ratio = INFERENCE_LONG_SIDE / max(width, height) if max(width, height) > INFERENCE_LONG_SIDE else 1.0
    return max(32, round(width * ratio / 32) * 32), max(32, round(height * ratio / 32) * 32)


def round_positive(value: float) -> int:
    return math.floor(value + 0.5)


class LocatorLab:
    def __init__(self, inputs: Path):
        self.inputs = inputs
        components = Path(os.environ["LOCALAPPDATA"]) / "screen-goated-toolbox" / "components"
        detector = installed_component(
            components,
            "screen-text-detector",
            "models/pp-ocr-screen-text/detector.onnx",
        )
        self.model_root = detector / "models/pp-ocr-screen-text"
        self.model = self.model_root / "detector.onnx"
        self.ocr_model = self.model_root / "recognizers/unified/model-cpu.ort"
        ocr_config = yaml.safe_load(
            (self.model_root / "recognizers/unified/config.yml").read_text(encoding="utf-8")
        )
        self.characters = [""] + ocr_config["PostProcess"]["character_dict"] + [" "]
        self.session = None
        self.ocr_session = None
        self.session_lock = threading.Lock()
        self.ocr_lock = threading.Lock()
        self.cache = OrderedDict()
        self.ocr_cache = {}

    def source(self, case: str) -> Path:
        path = self.inputs / case / "source.jpg"
        if not path.is_file():
            raise ValueError("Case has no source.jpg")
        return path

    def probability_map(self, case: str) -> dict:
        source = self.source(case)
        stat = source.stat()
        key = (stat.st_size, stat.st_mtime_ns)
        cached = self.cache.get(case)
        if cached and cached["key"] == key:
            self.cache.move_to_end(case)
            return cached
        disk_cache = source.parent / "locator-probability.npz"
        if disk_cache.is_file():
            with np.load(disk_cache) as stored:
                stored_key = tuple(int(value) for value in stored["source_key"])
                if stored_key == key:
                    cached = {
                        "key": key,
                        "image": np.asarray(Image.open(source).convert("RGB")),
                        "probabilities": stored["probabilities"],
                        "inference_ms": float(stored["inference_ms"]),
                    }
                    self.cache[case] = cached
                    self._bound_cache()
                    return cached
        image = np.asarray(Image.open(source).convert("RGB"))
        height, width = image.shape[:2]
        prepared_width, prepared_height = inference_size(width, height)
        resized = cv2.resize(image, (prepared_width, prepared_height), interpolation=cv2.INTER_LINEAR)
        bgr = resized[:, :, ::-1].astype(np.float32) / 255.0
        bgr = (bgr - np.array([0.485, 0.456, 0.406], dtype=np.float32)) / np.array(
            [0.229, 0.224, 0.225], dtype=np.float32
        )
        tensor = np.transpose(bgr, (2, 0, 1))[None, ...]
        with self.session_lock:
            if self.session is None:
                self.session = ort.InferenceSession(
                    str(self.model),
                    providers=["CPUExecutionProvider"],
                )
            started = time.perf_counter()
            output = self.session.run(None, {self.session.get_inputs()[0].name: tensor})[0]
            inference_ms = (time.perf_counter() - started) * 1000
        probabilities = np.asarray(output[0, 0], dtype=np.float32)
        temporary = source.parent / "locator-probability.npz.tmp"
        with temporary.open("wb") as file:
            np.savez_compressed(
                file,
                source_key=np.array(key, dtype=np.int64),
                probabilities=probabilities,
                inference_ms=np.array(inference_ms, dtype=np.float64),
            )
        os.replace(temporary, disk_cache)
        cached = {
            "key": key,
            "image": image,
            "probabilities": probabilities,
            "inference_ms": inference_ms,
        }
        self.cache[case] = cached
        self._bound_cache()
        return cached

    def _bound_cache(self) -> None:
        while len(self.cache) > MAX_CACHED_CASES:
            self.cache.popitem(last=False)

    def regions(self, case: str, pixel_threshold: float, box_threshold: float) -> dict:
        cached = self.probability_map(case)
        image = cached["image"]
        probabilities = cached["probabilities"]
        map_height, map_width = probabilities.shape
        image_height, image_width = image.shape[:2]
        mask = (probabilities > pixel_threshold).astype(np.uint8)
        count, labels, stats, _ = cv2.connectedComponentsWithStats(mask, connectivity=8)
        regions = []
        for label in range(1, count):
            if len(regions) >= MAX_REGIONS:
                break
            left, top, width, height, pixel_count = [int(value) for value in stats[label]]
            if min(width, height) < MIN_SIDE or pixel_count == 0:
                continue
            confidence = float(probabilities[labels == label].mean())
            if confidence < box_threshold:
                continue
            right = left + width - 1
            bottom = top + height - 1
            distance = width * height * UNCLIP_RATIO / (2.0 * (width + height))
            left = max(0, math.floor(left - distance))
            top = max(0, math.floor(top - distance))
            right = min(map_width - 1, math.ceil(right + distance))
            bottom = min(map_height - 1, math.ceil(bottom + distance))
            if min(right - left + 1, bottom - top + 1) < MIN_SIDE + 2:
                continue
            scaled = {
                "left": round_positive(left * image_width / map_width),
                "top": round_positive(top * image_height / map_height),
                "right": min(image_width, round_positive((right + 1) * image_width / map_width)),
                "bottom": min(image_height, round_positive((bottom + 1) * image_height / map_height)),
                "locatorConfidence": confidence,
            }
            if scaled["left"] < scaled["right"] and scaled["top"] < scaled["bottom"]:
                regions.extend(split_row(image, scaled))
        sort_reading_order(regions)
        return {
            "regions": regions,
            "mapSize": [map_width, map_height],
            "inferenceMs": round(cached["inference_ms"], 2),
        }

    def recognize(self, case: str, boxes: list[list[int]]) -> list[dict]:
        cached = self.probability_map(case)
        image = cached["image"]
        results = [None] * len(boxes)
        pending = []
        for index, box in enumerate(boxes):
            key = (case, cached["key"], tuple(box))
            if key in self.ocr_cache:
                results[index] = self.ocr_cache[key]
                continue
            left, top, width, height = box
            crop = image[top : top + height, left : left + width]
            candidates = [(crop, index, key)]
            if height > width * 3 // 2:
                candidates.extend([(np.rot90(crop, 1), index, key), (np.rot90(crop, 3), index, key)])
            pending.extend(candidates)
        if pending:
            recognized = self._recognize_images([candidate[0] for candidate in pending])
            grouped = {}
            for (_, index, key), recognition in zip(pending, recognized):
                grouped.setdefault((index, key), []).append(recognition)
            for (index, key), alternatives in grouped.items():
                alternatives.sort(key=recognition_quality, reverse=True)
                best = alternatives[0]
                self.ocr_cache[key] = best
                results[index] = best
        return results

    def _recognize_images(self, images: list[np.ndarray]) -> list[dict]:
        prepared = [prepare_text_line(image) for image in images]
        order = sorted(range(len(prepared)), key=lambda index: prepared[index][0])
        results = [None] * len(prepared)
        with self.ocr_lock:
            if self.ocr_session is None:
                self.ocr_session = ort.InferenceSession(
                    str(self.ocr_model),
                    providers=["CPUExecutionProvider"],
                )
            input_name = self.ocr_session.get_inputs()[0].name
            for start in range(0, len(order), 16):
                indices = order[start : start + 16]
                width = max(prepared[index][0] for index in indices)
                tensor = np.zeros((len(indices), 3, 48, width), dtype=np.float32)
                for batch_index, source_index in enumerate(indices):
                    source_width, chw = prepared[source_index]
                    tensor[batch_index, :, :, :source_width] = chw
                scores = self.ocr_session.run(None, {input_name: tensor})[0]
                for batch_index, source_index in enumerate(indices):
                    results[source_index] = decode_text(scores[batch_index], self.characters)
        return results


def split_row(image: np.ndarray, region: dict) -> list[dict]:
    left, top, right, bottom = [region[key] for key in ("left", "top", "right", "bottom")]
    width, height = right - left, bottom - top
    if height == 0 or width < height * 5:
        return [region]
    border = np.concatenate(
        (
            image[top, left:right],
            image[bottom - 1, left:right],
            image[top:bottom, left],
            image[top:bottom, right - 1],
        )
    )
    background = np.median(border, axis=0).astype(np.uint8)
    crop = image[top:bottom, left:right]
    distance = np.max(np.abs(crop.astype(np.int16) - background.astype(np.int16)), axis=2)
    required_ink = max(1, math.ceil(height / 12))
    occupied = np.count_nonzero(distance >= 36, axis=0) >= required_ink
    active = np.flatnonzero(occupied)
    if active.size == 0:
        return [region]
    first, last = int(active[0]), int(active[-1])
    minimum_gap = max(6, math.ceil(height / 2))
    minimum_segment = max(12, height)
    cuts = []
    gap_start = None
    for index in range(first, last + 1):
        if not occupied[index]:
            if gap_start is None:
                gap_start = index
        elif gap_start is not None:
            record_cut(cuts, gap_start, index, first, last, minimum_gap, minimum_segment)
            gap_start = None
    if gap_start is not None:
        record_cut(cuts, gap_start, last + 1, first, last, minimum_gap, minimum_segment)
    if not cuts:
        return [region]
    result = []
    start = first
    for end in cuts + [last + 1]:
        if end - start >= 4:
            split = dict(region)
            split["left"] = left + start
            split["right"] = left + min(end, width)
            result.append(split)
        start = end
    return result if len(result) > 1 else [region]


def record_cut(cuts, start, end, first, last, minimum_gap, minimum_segment):
    if end - start >= minimum_gap and start - first >= minimum_segment and last + 1 - end >= minimum_segment:
        cuts.append((start + end) // 2)


def sort_reading_order(regions: list[dict]) -> None:
    heights = sorted(max(1, region["bottom"] - region["top"]) for region in regions)
    row_quantum = max(4, heights[len(heights) // 2] if heights else 4) // 2
    regions.sort(
        key=lambda region: (
            region["top"] // row_quantum,
            region["left"],
            region["top"],
            region["bottom"],
            region["right"],
        )
    )


def prepare_text_line(source: np.ndarray) -> tuple[int, np.ndarray]:
    height, width = source.shape[:2]
    resized_width = min(1600, max(1, math.ceil(width * 48 / height)))
    input_width = min(1600, max(32, math.ceil(resized_width / 32) * 32))
    resized = cv2.resize(source, (resized_width, 48), interpolation=cv2.INTER_LINEAR)
    bgr = resized[:, :, ::-1].astype(np.float32) / 127.5 - 1.0
    chw = np.zeros((3, 48, input_width), dtype=np.float32)
    chw[:, :, :resized_width] = np.transpose(bgr, (2, 0, 1))
    return input_width, chw


def decode_text(scores: np.ndarray, characters: list[str]) -> dict:
    tokens = []
    confidence = 0.0
    count = 0
    previous = -1
    for row in scores:
        index = int(np.argmax(row))
        if index != 0 and index != previous and len(tokens) < 1024:
            tokens.append(characters[index])
            confidence += float(row[index])
            count += 1
        previous = index
    return {
        "text": "".join(tokens).strip(),
        "confidence": max(0.0, min(1.0, confidence / count if count else 0.0)),
    }


def recognition_quality(recognition: dict) -> float:
    useful = max(1, min(16, sum(character.isalnum() for character in recognition["text"])))
    return recognition["confidence"] * math.sqrt(useful)
