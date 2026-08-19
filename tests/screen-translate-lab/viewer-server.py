import json
import importlib.util
import os
import re
import sys
import threading
import webbrowser
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab_paths

ROOT = lab_paths.TOOLS
ARTIFACTS = lab_paths.artifact_root()
INPUTS = lab_paths.inputs_root()
ARTIFACT_PREFIXES = ("inputs", "archived-runs", "viewer-data.js")
CASE_NAME = re.compile(r"case-\d{2}")
WRITE_LOCK = threading.Lock()
LOCATOR_LOCK = threading.Lock()
PREVIEW_LOCK = threading.Lock()
PREVIEW_STATE = {"status": "idle"}

locator_spec = importlib.util.spec_from_file_location("locator_lab", ROOT / "locator-lab.py")
locator_module = importlib.util.module_from_spec(locator_spec)
locator_spec.loader.exec_module(locator_module)
LOCATOR_LAB = locator_module.LocatorLab(INPUTS)
preview_spec = importlib.util.spec_from_file_location("production_preview", ROOT / "production-preview.py")
preview_module = importlib.util.module_from_spec(preview_spec)
preview_spec.loader.exec_module(preview_module)


def case_directory(name: str) -> Path:
    if not CASE_NAME.fullmatch(name):
        raise ValueError("Invalid case name")
    directory = INPUTS / name
    if not directory.is_dir():
        raise ValueError("Unknown case")
    return directory


def read_comments(name: str) -> list[dict]:
    path = case_directory(name) / "comments.json"
    if not path.is_file():
        return []
    value = json.loads(path.read_text(encoding="utf-8"))
    return value if isinstance(value, list) else []


def read_raw_regions(name: str) -> list[dict]:
    value = json.loads((case_directory(name) / "detector-raw.json").read_text(encoding="utf-8"))
    if not isinstance(value, list):
        raise ValueError("Invalid detector raw data")
    return value


def overlap_score(left: list[int], right: list[int]) -> float:
    left_x, left_y, left_width, left_height = left
    right_x, right_y, right_width, right_height = right
    intersection_width = max(0, min(left_x + left_width, right_x + right_width) - max(left_x, right_x))
    intersection_height = max(0, min(left_y + left_height, right_y + right_height) - max(left_y, right_y))
    intersection = intersection_width * intersection_height
    if intersection == 0:
        return 0.0
    left_area = left_width * left_height
    right_area = right_width * right_height
    union = left_area + right_area - intersection
    return max(intersection / union, intersection / min(left_area, right_area))


def locator_regions(name: str, pixel: float, box: float, ocr: float, recognize: bool) -> dict:
    with LOCATOR_LOCK:
        result = LOCATOR_LAB.regions(name, pixel, box)
    raw = read_raw_regions(name)
    enriched = []
    for region in result["regions"]:
        bounds = [
            region["left"],
            region["top"],
            region["right"] - region["left"],
            region["bottom"] - region["top"],
        ]
        matches = [overlap_score(bounds, candidate["boxPx"]) for candidate in raw]
        best_index = max(range(len(matches)), key=matches.__getitem__) if matches else None
        matched = raw[best_index] if best_index is not None and matches[best_index] >= 0.45 else None
        candidates = [] if matched is None else [
            (matched.get("primaryText", ""), matched.get("primaryConfidence", 0.0)),
            *[
                (alternative.get("text", ""), alternative.get("confidence", 0.0))
                for alternative in matched.get("alternatives", [])
            ],
        ]
        passes_ocr = any(confidence >= ocr and any(character.isalpha() for character in text) for text, confidence in candidates)
        enriched.append(
            {
                "boxPx": bounds,
                "locatorConfidence": region["locatorConfidence"],
                "primaryText": matched.get("primaryText", "") if matched else "",
                "primaryConfidence": matched.get("primaryConfidence", 0.0) if matched else 0.0,
                "alternatives": matched.get("alternatives", []) if matched else [],
                "sourceRegion": best_index + 1 if matched else None,
                "ocrStatus": "cached" if matched else "pending",
                "passesOcr": passes_ocr,
            }
        )
    pending_indices = [index for index, region in enumerate(enriched) if region["ocrStatus"] == "pending"]
    if recognize and pending_indices:
        boxes = [enriched[index]["boxPx"] for index in pending_indices]
        with LOCATOR_LOCK:
            recognized = LOCATOR_LAB.recognize(name, boxes)
        for index, recognition in zip(pending_indices, recognized):
            enriched[index]["primaryText"] = recognition["text"]
            enriched[index]["primaryConfidence"] = recognition["confidence"]
            enriched[index]["ocrStatus"] = "lab"
    for region in enriched:
        candidates = [
            (region["primaryText"], region["primaryConfidence"]),
            *[
                (alternative.get("text", ""), alternative.get("confidence", 0.0))
                for alternative in region["alternatives"]
            ],
        ]
        region["passesOcr"] = any(
            confidence >= ocr and any(character.isalpha() for character in text)
            for text, confidence in candidates
        )
    result["regions"] = enriched
    result["recognizedCount"] = sum(region["ocrStatus"] != "pending" for region in enriched)
    result["acceptedCount"] = sum(region["passesOcr"] for region in enriched)
    return result


def last_preview(name: str) -> dict:
    directory = case_directory(name) / "production-preview"
    image = directory / "result.jpg"
    record_path = directory / "run.json"
    if not image.is_file() or not record_path.is_file():
        return {"available": False}
    record = json.loads(record_path.read_text(encoding="utf-8"))
    return {
        "available": True,
        "resultUrl": f"inputs/{name}/production-preview/result.jpg",
        "record": record,
        "modifiedNs": image.stat().st_mtime_ns,
    }


def run_preview(name: str) -> None:
    global PREVIEW_STATE
    try:
        result = preview_module.run(case_directory(name))
        with PREVIEW_LOCK:
            PREVIEW_STATE = {"status": "complete", "case": name, **result}
    except Exception as error:
        with PREVIEW_LOCK:
            PREVIEW_STATE = {"status": "error", "case": name, "error": str(error)}


def rerender_preview(name: str) -> None:
    global PREVIEW_STATE
    try:
        result = preview_module.rerender(case_directory(name))
        record = last_preview(name).get("record", {})
        with PREVIEW_LOCK:
            PREVIEW_STATE = {"status": "complete", "case": name, "record": record, **result}
    except Exception as error:
        with PREVIEW_LOCK:
            PREVIEW_STATE = {"status": "error", "case": name, "error": str(error)}


class ViewerHandler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT), **kwargs)

    def translate_path(self, path: str) -> str:
        head = urlparse(path).path.lstrip("/").split("/", 1)[0]
        if head in ARTIFACT_PREFIXES:
            return SimpleHTTPRequestHandler.translate_path(self, path).replace(
                str(ROOT), str(ARTIFACTS), 1
            )
        return SimpleHTTPRequestHandler.translate_path(self, path)

    def end_headers(self) -> None:
        self.send_header("Cache-Control", "no-store, no-cache, must-revalidate")
        self.send_header("Pragma", "no-cache")
        self.send_header("Expires", "0")
        super().end_headers()

    def send_json(self, status: int, value) -> None:
        body = json.dumps(value, ensure_ascii=False, indent=2).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path == "/api/comments":
            try:
                name = parse_qs(parsed.query).get("case", [""])[0]
                self.send_json(200, read_comments(name))
            except (ValueError, OSError, json.JSONDecodeError) as error:
                self.send_json(400, {"error": str(error)})
            return
        if parsed.path == "/api/locator-boxes":
            try:
                query = parse_qs(parsed.query)
                name = query.get("case", [""])[0]
                case_directory(name)
                pixel = float(query.get("pixel", ["0.15"])[0])
                box = float(query.get("box", ["0.8"])[0])
                ocr = float(query.get("ocr", ["0.5"])[0])
                recognize = query.get("recognize", ["0"])[0] == "1"
                if not all(0.0 <= value <= 1.0 for value in (pixel, box, ocr)):
                    raise ValueError("Thresholds must be between zero and one")
                self.send_json(200, locator_regions(name, pixel, box, ocr, recognize))
            except (ValueError, OSError, json.JSONDecodeError, RuntimeError) as error:
                self.send_json(400, {"error": str(error)})
            return
        if parsed.path == "/api/production-preview":
            try:
                query = parse_qs(parsed.query)
                name = query.get("case", [""])[0]
                case_directory(name)
                with PREVIEW_LOCK:
                    state = dict(PREVIEW_STATE)
                self.send_json(200, state if state.get("case") == name else {"status": "idle"})
            except ValueError as error:
                self.send_json(400, {"error": str(error)})
            return
        if parsed.path == "/api/production-preview/last":
            try:
                name = parse_qs(parsed.query).get("case", [""])[0]
                self.send_json(200, last_preview(name))
            except (ValueError, OSError, json.JSONDecodeError) as error:
                self.send_json(400, {"error": str(error)})
            return
        super().do_GET()

    def do_POST(self) -> None:
        if self.path == "/api/production-preview":
            self.start_production_preview()
            return
        if self.path != "/api/comments":
            self.send_json(404, {"error": "Not found"})
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if length <= 0 or length > 16 * 1024:
                raise ValueError("Invalid comment request size")
            request = json.loads(self.rfile.read(length))
            name = request.get("case", "")
            scope = request.get("scope", "region")
            region = request.get("region")
            region_key = request.get("regionKey")
            text = request.get("text", "").strip()
            if scope not in ("image", "region") or not text or len(text) > 4000:
                raise ValueError("Invalid comment")
            if scope == "region":
                valid_region = isinstance(region, int) and region >= 1
                valid_key = isinstance(region_key, str) and re.fullmatch(r"[A-Za-z0-9-]{1,80}", region_key)
                if not valid_region and not valid_key:
                    raise ValueError("Invalid region comment")
            directory = case_directory(name)
            if scope == "region" and region_key is None:
                regions = json.loads((directory / "detector-raw.json").read_text(encoding="utf-8"))
                if not isinstance(regions, list) or region > len(regions):
                    raise ValueError("Unknown detector region")
            path = directory / "comments.json"
            temporary = directory / "comments.json.tmp"
            with WRITE_LOCK:
                comments = read_comments(name)
                comment = {"scope": scope, "text": text}
                if scope == "region":
                    if region_key is not None:
                        comment["regionKey"] = region_key
                    else:
                        comment["region"] = region
                comments.append(comment)
                temporary.write_text(
                    json.dumps(comments, ensure_ascii=False, indent=2),
                    encoding="utf-8",
                )
                os.replace(temporary, path)
            self.send_json(201, comments[-1])
        except (ValueError, OSError, json.JSONDecodeError) as error:
            self.send_json(400, {"error": str(error)})

    def start_production_preview(self) -> None:
        global PREVIEW_STATE
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if length <= 0 or length > 1024:
                raise ValueError("Invalid preview request size")
            request = json.loads(self.rfile.read(length))
            name = request.get("case", "")
            action = request.get("action", "full")
            if action not in ("full", "rerender"):
                raise ValueError("Invalid preview action")
            case_directory(name)
            with PREVIEW_LOCK:
                if PREVIEW_STATE.get("status") == "running":
                    self.send_json(409, {"error": "A production preview is already running"})
                    return
                PREVIEW_STATE = {"status": "running", "case": name}
            target = rerender_preview if action == "rerender" else run_preview
            threading.Thread(target=target, args=(name,), daemon=True).start()
            self.send_json(202, dict(PREVIEW_STATE))
        except (ValueError, OSError, json.JSONDecodeError) as error:
            self.send_json(400, {"error": str(error)})

    def log_message(self, _format: str, *_args) -> None:
        pass


if __name__ == "__main__":
    server = ThreadingHTTPServer(("127.0.0.1", 8765), ViewerHandler)
    threading.Thread(target=preview_module.ensure_warm_host, daemon=True).start()
    webbrowser.open("http://127.0.0.1:8765/viewer.html")
    server.serve_forever()
