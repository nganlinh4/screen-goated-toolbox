#!/usr/bin/env python3
"""Enforce device Macrobenchmark budgets from AndroidX benchmark JSON output."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


def require_number(value: object, label: str) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise ValueError(f"{label} must be numeric")
    return float(value)


def benchmark_name(row: dict[str, Any]) -> str:
    return f"{row.get('className', '')}#{row.get('name', '')}"


def find_benchmark(rows: list[dict[str, Any]], suffix: str) -> dict[str, Any]:
    matches = [row for row in rows if suffix in benchmark_name(row)]
    if len(matches) != 1:
        names = [benchmark_name(row) for row in rows]
        raise ValueError(f"expected one benchmark containing {suffix!r}; found {names}")
    return matches[0]


def find_metric(row: dict[str, Any], name: str) -> dict[str, Any] | None:
    for collection_name in ("metrics", "sampledMetrics"):
        collection = row.get(collection_name, {})
        if not isinstance(collection, dict):
            continue
        for metric_name, value in collection.items():
            if metric_name.lower() == name.lower() and isinstance(value, dict):
                return value
    return None


def metric_value(
    row: dict[str, Any],
    metric_name: str,
    summary_name: str,
    *,
    required: bool = True,
) -> float | None:
    metric = find_metric(row, metric_name)
    if metric is None:
        if required:
            raise ValueError(f"{benchmark_name(row)} is missing {metric_name}")
        return None
    for key, value in metric.items():
        if key.lower() == summary_name.lower():
            return require_number(value, f"{metric_name}.{key}")
    if required:
        raise ValueError(
            f"{benchmark_name(row)} {metric_name} is missing {summary_name}"
        )
    return None


def load_benchmarks(paths: list[Path]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for path in paths:
        payload = json.loads(path.read_text(encoding="utf-8"))
        benchmarks = payload.get("benchmarks")
        if not isinstance(benchmarks, list):
            raise ValueError(f"{path} has no benchmarks array")
        rows.extend(row for row in benchmarks if isinstance(row, dict))
    return rows


def verify(paths: list[Path], contract_path: Path, diagnostic_only: bool) -> None:
    rows = load_benchmarks(paths)
    baseline = find_benchmark(rows, "coldStartupWithBaselineProfile")
    uncompiled = find_benchmark(rows, "coldStartupWithoutCompilation")

    contract = json.loads(contract_path.read_text(encoding="utf-8"))
    android = contract.get("android")
    if not isinstance(android, dict):
        raise ValueError("performance contract has no android object")
    budget = android.get("benchmark")
    if not isinstance(budget, dict):
        raise ValueError("performance contract has no android.benchmark object")

    baseline_startup = metric_value(baseline, "timeToInitialDisplayMs", "median")
    uncompiled_startup = metric_value(uncompiled, "timeToInitialDisplayMs", "median")
    assert baseline_startup is not None and uncompiled_startup is not None
    ratio = baseline_startup / uncompiled_startup

    frame_p95 = metric_value(baseline, "frameDurationCpuMs", "P95")
    overrun_p95 = metric_value(
        baseline, "frameOverrunMs", "P95", required=False
    )
    rss_anon = metric_value(baseline, "memoryRssAnonKb", "maximum")
    heap_size = metric_value(baseline, "memoryHeapSizeKb", "maximum")

    summary = (
        f"startup={baseline_startup:.1f}ms vs {uncompiled_startup:.1f}ms "
        f"({ratio:.3f}x), frameP95={frame_p95:.1f}ms, "
        f"rssAnonMax={rss_anon:.0f}KiB, heapMax={heap_size:.0f}KiB"
    )
    if diagnostic_only:
        print(f"Android benchmark diagnostic: {summary}")
        return

    limits = {
        "maxBaselineStartupRatio": ratio,
        "maxFrameDurationCpuP95Ms": frame_p95,
        "maxMemoryRssAnonKb": rss_anon,
        "maxMemoryHeapSizeKb": heap_size,
    }
    if overrun_p95 is not None:
        limits["maxFrameOverrunP95Ms"] = overrun_p95
    failures = []
    for name, observed in limits.items():
        allowed = require_number(budget.get(name), f"android.benchmark.{name}")
        if observed > allowed:
            failures.append(f"{name}={observed:.3f} exceeds {allowed:.3f}")
    if failures:
        raise ValueError("; ".join(failures))
    print(f"Android benchmark passed: {summary}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results", required=True, type=Path, nargs="+")
    parser.add_argument("--contract", required=True, type=Path)
    parser.add_argument("--diagnostic-only", action="store_true")
    args = parser.parse_args()
    try:
        verify(
            [path.resolve() for path in args.results],
            args.contract.resolve(),
            args.diagnostic_only,
        )
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"Android benchmark failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
