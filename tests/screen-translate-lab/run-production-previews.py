"""Run the production preview for every lab case and summarize the results.

Each case is translated end to end by the development build through its lab
queue, so this needs an interactive desktop session and takes over the screen
while it runs. Results land in each case's production-preview directory and a
summary is written next to the case inputs.
"""

from __future__ import annotations

import importlib.util
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab_paths

TOOLS = lab_paths.TOOLS
spec = importlib.util.spec_from_file_location(
    "production_preview", TOOLS / "production-preview.py"
)
production_preview = importlib.util.module_from_spec(spec)
spec.loader.exec_module(production_preview)

SUMMARY_NAME = "all-cases-production-result.json"


def phase_ms(record: dict, phase: str) -> float | None:
    for timing in record.get("timingsMs", []):
        if timing.get("phase") == phase:
            return timing.get("elapsedMs")
    return None


def summarize(name: str, result: dict) -> dict:
    record = result["record"]
    return {
        "case": name,
        "status": "complete",
        "elapsedMs": result["elapsedMs"],
        "regions": record.get("renderedRegionCount", 0),
        "detectorMs": phase_ms(record, "detector_complete"),
        "firstPaintMs": phase_ms(record, "first_painted"),
        "completeMs": phase_ms(record, "final_painted"),
    }


def attempt(case: Path, attempts: int, pace: float) -> dict:
    for round_index in range(1, attempts + 1):
        try:
            result = production_preview.run(case)
            entry = summarize(case.name, result)
            print(
                f"    complete in {entry['elapsedMs']} ms, {entry['regions']} regions",
                flush=True,
            )
            return entry
        except Exception as error:
            print(f"    attempt {round_index}/{attempts} failed: {error}", flush=True)
            if round_index == attempts:
                return {"case": case.name, "status": "error", "error": str(error)}
            print(f"    waiting {round(pace)} s before retrying", flush=True)
            time.sleep(pace)
    raise AssertionError("unreachable")


def merge(previous: list[dict], summary: list[dict]) -> list[dict]:
    merged = {entry["case"]: entry for entry in previous}
    merged.update({entry["case"]: entry for entry in summary})
    return [merged[name] for name in sorted(merged)]


def main() -> int:
    arguments = sys.argv[1:]
    attempts = 1
    pace = 15.0
    names = []
    while arguments:
        item = arguments.pop(0)
        if item == "--attempts":
            attempts = int(arguments.pop(0))
        elif item == "--pace":
            pace = float(arguments.pop(0))
        else:
            names.append(item)

    inputs = lab_paths.inputs_root()
    cases = sorted(
        path
        for path in inputs.iterdir()
        if path.is_dir() and (path / "source.jpg").is_file()
    )
    if names:
        selected = {name for name in names}
        cases = [case for case in cases if case.name in selected]
        missing = selected - {case.name for case in cases}
        if missing:
            print(f"Unknown case(s): {', '.join(sorted(missing))}")
            return 1
    if not cases:
        print(f"No case folders contain source.jpg under {inputs}")
        return 1

    summary = []
    started = time.time()
    try:
        for index, case in enumerate(cases, start=1):
            print(f"[{index}/{len(cases)}] {case.name} ...", flush=True)
            summary.append(attempt(case, attempts, pace))
            if pace and index < len(cases):
                time.sleep(pace)
    finally:
        host = production_preview.HOST_PROCESS
        if host is not None and host.poll() is None:
            host.terminate()
            try:
                host.wait(timeout=5)
            except Exception:
                host.kill()

    destination = lab_paths.artifact_root() / SUMMARY_NAME
    if names and destination.is_file():
        summary = merge(json.loads(destination.read_text(encoding="utf-8")), summary)
    destination.write_text(
        json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    complete = sum(1 for entry in summary if entry["status"] == "complete")
    print(
        f"{complete}/{len(summary)} cases complete in {round(time.time() - started)} s",
        flush=True,
    )
    return 0 if complete == len(summary) else 1


if __name__ == "__main__":
    raise SystemExit(main())
