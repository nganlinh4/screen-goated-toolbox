# Catalog benchmark

This opt-in Rust benchmark exercises the production catalog and provider request paths. It measures text translation, image coordinate grounding, and image OCR. Each suite has ten cases of increasing difficulty. Scheduling is round-major: every selected model sees difficulty 1 before any model moves to difficulty 2.

Normal `cargo test` does not call providers. It validates the manifest, all image decodes, difficulty coverage, coordinate bounds, and OCR crop bounds. Open `review.html` before the first live run and check all twenty model inputs, the ten red coordinate boxes, both OCR crops, and OCR references in `manifest.json`.

## Live run

At least one matching provider credential must be in the environment or saved app config. A live run requires an explicit opt-in:

```powershell
$env:CATALOG_BENCH_LIVE = "1"
$env:CATALOG_BENCH_MODELS = "groq-qwen-3-6-27b-vision,google-gemini-3-5-flash-lite-vision"
cargo test catalog_benchmark_live -- --ignored --nocapture
```

Omit `CATALOG_BENCH_MODELS` to select every enabled catalog model that has usable credentials. Optional controls:

- `CATALOG_BENCH_SUITES=text,coordinate,ocr`
- `CATALOG_BENCH_MIN_INTERVAL_MS=2500` (per provider)
- `CATALOG_BENCH_REQUEST_TIMEOUT_SECS=120`
- `CATALOG_BENCH_OUTPUT=target/catalog-benchmark/my-run`
- `CATALOG_BENCH_RESUME_INPUTS=target/catalog-benchmark/interrupted/attempts.jsonl` skips successful cells already present in one or more semicolon-separated reports

The output directory contains `attempts.jsonl`, `summary.json`, and `summary.md`. The raw output and rubric are retained because translation accuracy needs human judgment; the automatic translation score combines reference similarity with explicit terminology, placeholder, forbidden-term, and line-count constraints. Coordinate accuracy is strict box hit rate. OCR accuracy is the best normalized character similarity across the primary and any layout-equivalent accepted references. Three OCR cases use the production OCR preset prompt verbatim; two of those cases apply deterministic manifest-defined crops before entering the production vision request path.

Focused reruns can replace failed cells in an earlier report without repeating successful provider calls. Inputs are applied left-to-right, with the latest matching model/suite/case/round winning:

```powershell
$env:CATALOG_BENCH_MERGE_INPUTS = "target/catalog-benchmark/base/attempts.jsonl;target/catalog-benchmark/recovery/attempts.jsonl"
$env:CATALOG_BENCH_OUTPUT = "target/catalog-benchmark/complete"
cargo test catalog_benchmark_merge_reports -- --ignored --nocapture
```

The runner makes one request per attempt and performs no benchmark-level retry. Errors—including overload responses—are recorded so availability is part of consistency. The production Gemini transport retries generic transient HTTP 429/500/502/503/504 responses at most twice with short jittered backoff. An explicit provider retry delay above eight seconds fails fast so the application's model fallback chain can advance. Final Gemini errors retain the structured provider message, status, retry delay, and quota metadata in `attempts.jsonl`.
