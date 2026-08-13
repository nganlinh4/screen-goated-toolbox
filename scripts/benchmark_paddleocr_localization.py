#!/usr/bin/env python3
"""Run a persistent PaddleOCR worker against screen-localization fixtures."""

from __future__ import annotations

import argparse
import difflib
import html
import json
import time
from pathlib import Path

from PIL import Image, ImageDraw
from paddleocr import PaddleOCR


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--det-model-dir", type=Path, required=True)
    parser.add_argument("--rec-model-dir", type=Path, required=True)
    parser.add_argument("--det-model-name", default="PP-OCRv5_mobile_det")
    parser.add_argument("--rec-model-name", default="en_PP-OCRv5_mobile_rec")
    parser.add_argument(
        "--engine",
        choices=("paddle_static", "onnxruntime"),
        default="paddle_static",
    )
    parser.add_argument("--cpu-threads", type=int, default=8)
    parser.add_argument("--recognition-batch-size", type=int, default=6)
    parser.add_argument("--confidence", type=float, default=0.5)
    return parser.parse_args()


def normalized_text(value: str) -> str:
    return " ".join(value.casefold().split())


def similarity(left: str, right: str) -> float:
    return difflib.SequenceMatcher(
        None, normalized_text(left), normalized_text(right)
    ).ratio()


def polygon_box(points: list[list[int]]) -> list[int]:
    xs = [point[0] for point in points]
    ys = [point[1] for point in points]
    left, top, right, bottom = min(xs), min(ys), max(xs), max(ys)
    return [left, top, max(1, right - left), max(1, bottom - top)]


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


def coverage(gold: list[int], predicted: list[int]) -> float:
    return intersection(gold, predicted) / max(1, area(gold))


def overpaint(gold: list[int], predicted: list[int]) -> float:
    return (area(predicted) - intersection(gold, predicted)) / max(1, area(gold))


def paint_box(box: list[int], width: int, height: int) -> list[int]:
    horizontal = min(3, max(1, (box[3] + 7) // 8))
    vertical = min(4, max(2, (box[3] + 3) // 4))
    left = max(0, box[0] - horizontal)
    top = max(0, box[1] - vertical)
    right = min(width, box[0] + box[2] + horizontal)
    bottom = min(height, box[1] + box[3] + vertical)
    return [left, top, max(1, right - left), max(1, bottom - top)]


def keep_region(text: str, confidence: float, threshold: float) -> bool:
    return confidence >= threshold and any(character.isalpha() for character in text)


def evaluate(case: dict, predictions: list[dict], width: int, height: int) -> dict:
    candidates = []
    for gold_index, gold in enumerate(case["regions"]):
        for prediction_index, prediction in enumerate(predictions):
            text_score = similarity(gold["source_text"], prediction["source_text"])
            if text_score < 0.55:
                continue
            candidates.append(
                (
                    0.75 * text_score + 0.25 * iou(gold["box_px"], prediction["box_px"]),
                    gold_index,
                    prediction_index,
                    text_score,
                )
            )
    candidates.sort(reverse=True)
    used_gold: set[int] = set()
    used_predictions: set[int] = set()
    matches = []
    for _, gold_index, prediction_index, text_score in candidates:
        if gold_index in used_gold or prediction_index in used_predictions:
            continue
        used_gold.add(gold_index)
        used_predictions.add(prediction_index)
        gold = case["regions"][gold_index]["box_px"]
        raw = predictions[prediction_index]["box_px"]
        painted = paint_box(raw, width, height)
        matches.append(
            {
                "gold_index": gold_index,
                "prediction_index": prediction_index,
                "text_similarity": text_score,
                "raw_iou": iou(gold, raw),
                "raw_coverage": coverage(gold, raw),
                "raw_overpaint": overpaint(gold, raw),
                "painted_iou": iou(gold, painted),
                "painted_coverage": coverage(gold, painted),
                "painted_overpaint": overpaint(gold, painted),
            }
        )

    def mean(field: str) -> float:
        return sum(match[field] for match in matches) / max(1, len(matches))

    recall = len(matches) / max(1, len(case["regions"]))
    precision = len(matches) / max(1, len(predictions))
    f1 = 2 * recall * precision / max(0.000001, recall + precision)
    return {
        "expected_regions": len(case["regions"]),
        "predicted_regions": len(predictions),
        "matched_regions": len(matches),
        "region_recall": recall,
        "region_precision": precision,
        "region_f1": f1,
        "raw_mean_iou": mean("raw_iou"),
        "raw_mean_coverage": mean("raw_coverage"),
        "raw_mean_overpaint": mean("raw_overpaint"),
        "painted_mean_iou": mean("painted_iou"),
        "painted_mean_coverage": mean("painted_coverage"),
        "painted_mean_overpaint": mean("painted_overpaint"),
        "matches": matches,
    }


def draw_overlay(
    source: Image.Image, gold: list[list[int]], predicted: list[list[int]], output: Path
) -> None:
    image = source.convert("RGBA")
    painter = ImageDraw.Draw(image)
    thickness = max(2, min(5, max(image.size) // 700))
    for box, color in [(box, "#00ff70") for box in gold] + [
        (box, "#00d2ff") for box in predicted
    ]:
        x, y, width, height = box
        painter.rectangle(
            [x, y, x + width - 1, y + height - 1], outline=color, width=thickness
        )
    image.save(output)


def result_payload(result: object) -> dict:
    payload = result.json
    return payload.get("res", payload)


def main() -> None:
    args = parse_args()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    cases = manifest["localization_cases"]
    image_root = args.manifest.parent
    args.output.mkdir(parents=True, exist_ok=True)

    started = time.perf_counter()
    ocr_options = {
        "engine": args.engine,
        "text_detection_model_name": args.det_model_name,
        "text_detection_model_dir": str(args.det_model_dir.resolve()),
        "text_recognition_model_name": args.rec_model_name,
        "text_recognition_model_dir": str(args.rec_model_dir.resolve()),
        "text_recognition_batch_size": args.recognition_batch_size,
        "use_doc_orientation_classify": False,
        "use_doc_unwarping": False,
        "use_textline_orientation": False,
        "return_word_box": True,
        "device": "cpu",
        "cpu_threads": args.cpu_threads,
    }
    if args.engine == "paddle_static":
        ocr_options["enable_mkldnn"] = False
    ocr = PaddleOCR(**ocr_options)
    initialization_ms = round((time.perf_counter() - started) * 1000, 2)

    warmup_path = image_root / cases[0]["image"]
    warmup_started = time.perf_counter()
    list(ocr.predict(str(warmup_path)))
    warmup_ms = round((time.perf_counter() - warmup_started) * 1000, 2)

    reports = []
    for case in cases:
        image_path = image_root / case["image"]
        source = Image.open(image_path)
        predict_started = time.perf_counter()
        results = list(ocr.predict(str(image_path)))
        latency_ms = round((time.perf_counter() - predict_started) * 1000, 2)
        if len(results) != 1:
            raise RuntimeError(f"expected one Paddle result for {case['id']}")
        payload = result_payload(results[0])
        predictions = []
        for text, confidence, polygon in zip(
            payload["rec_texts"], payload["rec_scores"], payload["rec_polys"]
        ):
            confidence = float(confidence)
            if keep_region(text, confidence, args.confidence):
                predictions.append(
                    {
                        "source_text": text,
                        "confidence": confidence,
                        "polygon_px": polygon,
                        "box_px": polygon_box(polygon),
                    }
                )
        metrics = evaluate(case, predictions, source.width, source.height)
        raw_path = args.output / f"{case['id']}-raw.png"
        paint_path = args.output / f"{case['id']}-painted.png"
        gold = [region["box_px"] for region in case["regions"]]
        draw_overlay(source, gold, [item["box_px"] for item in predictions], raw_path)
        draw_overlay(
            source,
            gold,
            [paint_box(item["box_px"], source.width, source.height) for item in predictions],
            paint_path,
        )
        reports.append(
            {
                "case_id": case["id"],
                "difficulty": case["difficulty"],
                "latency_ms": latency_ms,
                "image_width": source.width,
                "image_height": source.height,
                "predictions": predictions,
                "metrics": metrics,
                "raw_overlay": raw_path.name,
                "painted_overlay": paint_path.name,
            }
        )

    summary = {
        "engine": f"PaddleOCR/{args.engine}",
        "detection_model": args.det_model_name,
        "recognition_model": args.rec_model_name,
        "initialization_ms": initialization_ms,
        "warmup_ms": warmup_ms,
        "confidence_threshold": args.confidence,
        "recognition_batch_size": args.recognition_batch_size,
        "cases": reports,
    }
    (args.output / "summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    cards = []
    for report in reports:
        metrics = report["metrics"]
        cards.append(
            f"<article><h2>Level {report['difficulty']} · {html.escape(report['case_id'])}</h2>"
            f"<div><img src='{report['raw_overlay']}'><img src='{report['painted_overlay']}'></div>"
            f"<p>{report['latency_ms']:.1f} ms · matched {metrics['matched_regions']}/"
            f"{metrics['expected_regions']} · predicted {metrics['predicted_regions']} · "
            f"raw IoU {metrics['raw_mean_iou']:.1%} · coverage "
            f"{metrics['raw_mean_coverage']:.1%} · painted overpaint "
            f"{metrics['painted_mean_overpaint']:.2f}×</p></article>"
        )
    review = (
        "<!doctype html><meta charset='utf-8'><title>PaddleOCR localization</title>"
        "<style>body{font:14px system-ui;background:#15171b;color:#edf1f5;margin:24px}"
        "article{margin:24px 0;padding:16px;background:#22262d;border-radius:12px}"
        "article div{display:grid;grid-template-columns:1fr 1fr;gap:12px}img{width:100%}</style>"
        f"<h1>PaddleOCR localization probe</h1><p>Initialization {initialization_ms:.1f} ms; "
        f"warmup {warmup_ms:.1f} ms.</p>{''.join(cards)}"
    )
    (args.output / "review.html").write_text(review, encoding="utf-8")
    print(json.dumps(summary, ensure_ascii=True))


if __name__ == "__main__":
    main()
