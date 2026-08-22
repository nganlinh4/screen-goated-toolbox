# Catalog benchmark

Latest completed catalog decision record for OCR and the image chain order:
[`RESULTS-2026-08-20-PROTOCOL11-CLEAN.md`](RESULTS-2026-08-20-PROTOCOL11-CLEAN.md).

Superseded OCR record:
[`RESULTS-2026-08-20-PROTOCOL11-OCR.md`](RESULTS-2026-08-20-PROTOCOL11-OCR.md), which
discharges the re-run the protocol-10 amendment required and owns every OCR latency
and accuracy figure. It reordered no chain: that run hit a shared Gemini quota
exhaustion, so its reliability column is an artifact.

Existing chain orders, text and coordinate rows:
[`RESULTS-2026-08-19-PROTOCOL10.md`](RESULTS-2026-08-19-PROTOCOL10.md), the source
of every chain order, default and latency. Amended on 2026-08-20: its OCR accuracy
figures were measured through a request shape the product no longer sends, so
`benchmark_protocol_version` was 11 and every OCR row needed a fresh run. Latency,
reliability, the text and coordinate suites and all chain orders still stood at
that amendment. Protocol 13 now supersedes its text rows.
The interim decisions it absorbs remain in
[`RESULTS-2026-08-19-FOLLOWUP.md`](RESULTS-2026-08-19-FOLLOWUP.md), and the run those
rested on is
[`RESULTS-2026-08-18-PROTOCOL9.md`](RESULTS-2026-08-18-PROTOCOL9.md).
The preceding protocol-9 decision remains in
[`RESULTS-2026-08-10-PROTOCOL9.md`](RESULTS-2026-08-10-PROTOCOL9.md).
The preceding protocol-7 decision remains in
[`RESULTS-2026-08-06-PROTOCOL7.md`](RESULTS-2026-08-06-PROTOCOL7.md).
The immediately preceding text-candidate and vision-tail decisions remain in
[`RESULTS-2026-08-06-TEXT-CANDIDATES.md`](RESULTS-2026-08-06-TEXT-CANDIDATES.md)
and [`RESULTS-2026-07-26-VISION-TAIL-DIAGNOSTIC.md`](RESULTS-2026-07-26-VISION-TAIL-DIAGNOSTIC.md).
Its baseline full-catalog run is
[`RESULTS-2026-07-26-PROTOCOL6.md`](RESULTS-2026-07-26-PROTOCOL6.md).
Earlier protocol-5 and discovery records remain available in
[`RESULTS-2026-07-24-R3.md`](RESULTS-2026-07-24-R3.md),
[`RESULTS-2026-07-24-OPENROUTER-NEMOTRON.md`](RESULTS-2026-07-24-OPENROUTER-NEMOTRON.md).
The earlier free-model discovery passes are recorded in
[`OPENROUTER-SHORTLIST-2026-07-24.md`](OPENROUTER-SHORTLIST-2026-07-24.md).
Their completed live candidate evaluation is recorded in
[`RESULTS-2026-07-24-OPENROUTER-SHORTLIST.md`](RESULTS-2026-07-24-OPENROUTER-SHORTLIST.md).
The post-Ling free-route screen is recorded in
[`OPENROUTER-SCREEN-2026-07-24-R2.md`](OPENROUTER-SCREEN-2026-07-24-R2.md).

Protocol 13 supersedes every earlier text row. Existing catalog values remain the
last reviewed decisions, but a new text result is catalog-ready only after a
complete protocol-13 run and its structured human review.

This opt-in Rust benchmark exercises the production catalog and provider request paths. It measures diverse text transformation, image coordinate grounding, and image OCR. The ten text levels cover classification, extraction, translation, rewriting, summarization, structured extraction, reasoning, and synthesis; retained translation coverage includes Korean or mixed Korean to Vietnamese and Simplified Chinese to Vietnamese. Each catalog-history suite has ten cases of increasing difficulty. A separate three-level Screen Translate diagnostic tests the production structured text contract across the configured text-priority models; it never contributes to catalog history. Scheduling is round-major: every selected model sees difficulty 1 before any model moves to difficulty 2. Coordinate attempts reuse Computer Control's exact point prompt, schema, tolerant parser, 1600 px short-edge JPEG preparation, crosshair crop, verification prompt/schema/parser, and 70% acceptance threshold.

Normal `cargo test` does not call providers. It validates the manifest, all image decodes, difficulty coverage, coordinate bounds, OCR crop bounds, each OCR input origin, and every localization golden. Open `review.html` before the first live run and check the image inputs, the ten red coordinate boxes and zooms, the green localization regions, both OCR crops, and OCR references in `manifest.json`.

## Live run

At least one matching provider credential must be in the environment or saved app config. A live run requires an explicit opt-in:

Gemini, Groq and OpenRouter benchmarks discover indexed credentials and rotate
them once per provider call in stable numeric order. A second Groq account was
verified to carry its own allowance: draining one key's token balance left the
other near full, with an independent request counter, so the pools multiply the
free tier rather than sharing it. Keep the existing primary names
for compatibility and add indexed names such as `GEMINI_API_KEY_2` or
`OPENROUTER_API_KEY_2`. Blank, duplicate, and noncanonical indexed names are
ignored. Rotation is benchmark-only; the installed application continues to
use its single configured credential. Coordinate locate and verification calls
each advance the pool independently. The benchmark reads these slots directly
from the repository `.env`; a nonblank process environment value wins over the
matching file value, and only the primary slot may fall back to saved app
configuration.

```powershell
$env:CATALOG_BENCH_LIVE = "1"
$env:CATALOG_BENCH_MODELS = "groq-qwen-3-6-27b-vision,google-gemini-3-5-flash-lite-vision"
cargo test catalog_benchmark_live -- --ignored --nocapture
```

Omit `CATALOG_BENCH_MODELS` to select every enabled catalog model that has usable credentials. Optional controls:

- `CATALOG_BENCH_SUITES=text,coordinate,ocr`
- `CATALOG_BENCH_PROVIDERS=google,groq,nvidia` selects providers while retaining signed-feed discovery
- `CATALOG_BENCH_MIN_INTERVAL_MS=2500` (per provider)
- `CATALOG_BENCH_REQUEST_TIMEOUT_SECS=120`
- `CATALOG_BENCH_OUTPUT=target/catalog-benchmark/my-run`
- `CATALOG_BENCH_HISTORY_ROOT=target/catalog-benchmark` changes the local history root
- `CATALOG_BENCH_RESUME_INPUTS=target/catalog-benchmark/interrupted/attempts.jsonl` skips successful cells already present in one or more semicolon-separated reports
- `GEMINI_API_KEYS_JSON`, `GROQ_API_KEYS_JSON`, `OPENROUTER_API_KEYS_JSON`, and `NVIDIA_API_KEYS_JSON` accept JSON arrays of independent credentials; indexed names remain supported

Without an explicit output, runs are stored under
`target/catalog-benchmark/runs/`. A completed live-run directory contains
`attempts.jsonl`, `summary.json`, `summary.md`, `review-template.json`,
`human-review.md`, and `run.json`. The raw output and rubric are retained because
valid text answers can differ substantially. Reference similarity, terminology,
placeholder, forbidden-term, and line-count metrics appear only in attempt
details to prioritize review; they do not become text quality scores.
Coordinate accuracy is strict end-to-end product success: the located point
must hit the reviewed box and the production crosshair verifier must accept it
at 70% confidence or higher. Locator-only hit and verifier results remain in
the attempt details for diagnosis. OCR accuracy is the best normalized
character similarity across the primary and any layout-equivalent accepted
references. OCR normalization collapses layout whitespace and treats straight
and curly quotation marks as equivalent, while preserving case, punctuation,
and diacritics. Three OCR cases use the production OCR preset prompt verbatim;
two of those cases apply deterministic manifest-defined crops before entering
the production vision request path. They are difficulty levels 3–5; the ten OCR
levels now rise from large daily text through tiny UI text and dense layouts to
degraded newspaper print and handwriting.

Each OCR case also declares its production input origin. `screen-crop-png`
re-encodes the selected pixels as PNG, matching Windows region capture and
clipboard-image processing. `original-file` preserves the fixture bytes,
matching the dropped-file path and Google's original-byte zero-copy behavior.
Original-file cases cannot also define a crop. The current mix deliberately
covers six screen selections and four dropped photos/scans instead of letting a
fixture's filename accidentally decide its wire encoding.

## Transport-policy probe

Before changing a production vision request profile, compare candidate wire
shapes with the opt-in probe. Probe output is diagnostic JSONL and is never
registered in catalog benchmark history:

```powershell
$env:CATALOG_BENCH_TRANSPORT_PROBE = "1"
$env:CATALOG_BENCH_PROBE_MODELS = "google-gemma-4-31b-vision"
$env:CATALOG_BENCH_PROBE_CASES = "3,4,10"
$env:CATALOG_BENCH_PROBE_VARIANTS = "text-default,image-default,image-low,image-medium,image-high,image-default-stream"
$env:CATALOG_BENCH_PROBE_OUTPUT = "target/catalog-benchmark/transport-probe.jsonl"
cargo test catalog_benchmark_transport_probe -- --ignored --nocapture
```

`CATALOG_BENCH_PROBE_PROMPT_OVERRIDE` can isolate prompt wording. Every record
keeps the exact prompt, full-result latency, OCR score, response, and raw
Gemini usage metadata. Use it to test competing explanations such as input
order, image-token resolution, streaming, or prompt length before changing
`catalog/model_catalog.json#vision_request_profiles`.

Every live request calls the same Rust `translate_text_streaming` or
`translate_image_streaming` entry point as the app. That shared path owns
reasoning controls, request profiles, provider retries, parsing, and repetition
protection. Benchmark day starts by refreshing the same verified signed NVIDIA
feed, so runnable curated and newly discovered NVIDIA endpoints use the same
current request control as normal SGT operation. First-party documentation and
the production payload must be rechecked during human review; if best practice
changes, fix the shared adapter/profile and rerun under a new protocol.

## Local latest-run history

[`history-policy.json`](history-policy.json) is the machine-readable global
policy. After each completed live run, the helper refreshes
`target/catalog-benchmark/latest.json` and `latest.md`. Everything under
`target/` is local and Git-ignored; commit only a reviewed, compact dated
decision record.

A model/suite row becomes catalog-ready from the newest complete live run with
the current benchmark fixture fingerprint, durable model ID, provider API
endpoint, effective reasoning policy, and benchmark protocol version. The
fixture fingerprint covers the manifest and exact image bytes. Completeness
requires exactly one attempt for every current case, including failed requests.
An incomplete newer run cannot displace an older complete row.

Increment `benchmark_protocol_version` whenever scoring or benchmark request
semantics change. Old runs remain stored for audit but never contribute to
current values. The whole-catalog fingerprint is recorded for audit but does
not invalidate results when unrelated display metadata changes. Interrupted
runs have no `run.json`. Focused recoveries may be merged with their base output
and registered once as one complete logical run; never register the fragments
as separate results.

Protocol 13 retains protocol 12's diverse explicit text tasks and changes the
decision authority: text automatic metrics are triage only, complete human
reviews own quality, and every recorded latency is start-to-full-result only.
Protocol 12 replaced the translation-locked text request with explicit task
instructions spanning eight task families. It retains Korean and Simplified
Chinese translation into Vietnamese, starts with a several-word classification,
and ends with a long structured policy synthesis. Dedicated non-LLM and
search-by-default services are excluded because they do not execute the normal
general text contract. The changed
fixture fingerprint, scoring weights, and protocol version prevent older text
rows from being combined with these results.

## Human review

Copy `review-template.json` to `reviews.json`. For every successful response that
requires review, set `verdict` to `pass`, `partial`, or `fail`; set a 1–5
`rating`; answer every `rubric_checks` item; and add notes where useful. Refresh
or register history after saving the file. A missing verdict, rating, or rubric
judgment keeps the row explicitly not decision-ready.

## Hosted parallel run

Dispatch `.github/workflows/catalog-benchmark-day.yml` for the shortest wall
time. It compiles one Windows test binary, shards model-scoped providers across
GitHub-hosted runners, keeps OpenRouter provider-wide, runs NVIDIA from the
verified signed feed, merges every fragment, and uploads
`catalog-benchmark-complete-human-review`. Missing provider secrets skip only
that provider. JSON credential arrays rotate independent accounts/projects and
start at shard-specific offsets.

The plan job first runs `scripts/catalog_model_discovery.py`. Gemini and Groq
inventories come from their authenticated list-model APIs across every configured
credential; public first-party pricing/free-limit pages provide separate tier
evidence. The final artifact includes `catalog-model-discovery.json`. Discovery
never mutates the catalog and never treats API visibility as proof that an
endpoint is free-tier or general purpose. Routine discovery is browserless;
browser review is reserved for an API/document disagreement, an unparseable page,
an ambiguous lifecycle/capability, or an authenticated project-limit check.

Set the workflow's optional `resume_run_id` to a prior benchmark-day run to reuse
its successful cells and rerun only incomplete work. The merge job includes that
base run before recovery fragments, so the final artifact remains one complete
logical run. Groq vision shards reserve their documented free-tier token capacity
with a longer interval; this avoids misclassifying predictable quota admission as
model failure while independent credentials still rotate.

For the selected latest row:

- text accuracy comes only from completed structured human review; automatic
  metrics remain visible as triage evidence;
- reliability is successful attempts divided by every attempt, including
  overload and quota errors (ten attempts per suite);
- text `catalog_latency_ms` is the median completion time across that run's ten
  levels;
- vision `catalog_latency_ms` uses the OCR row and is the median over successful
  inputs whose effective longest edge is at
  most 1024 px; current fixtures provide five OCR cases in that representative
  cohort;
- catalog P95 describes the representative cohort, while explicit all-case
  median and P95 retain the large-image stress evidence.

Coordinate rows remain separate Computer Control evidence and must not be mixed
into the general image-to-text latency shown for OCR-oriented features. Windows,
Android Phone Control, and this benchmark share the strict JSON point contract
and fresh-crosshair verification gate; platform capture remains a thin adapter.

Refresh local history without provider calls:

```powershell
cargo test catalog_benchmark_refresh_latest_history -- --ignored --nocapture
```

Register a completed live report, or a base plus focused recoveries already
merged into one complete logical report:

```powershell
$env:CATALOG_BENCH_REGISTER_OUTPUT = "target/catalog-benchmark/my-old-live-run"
cargo test catalog_benchmark_register_history_run -- --ignored --nocapture
```

If outputs live outside the default root, set `CATALOG_BENCH_HISTORY_ROOT` to
their common parent before refreshing. Registration is idempotent once
`run.json` exists.

## Reading speed correctly

Every request uses the matching production transport mode and records:

- the effective catalog-owned ordinary reasoning policy;
- the generated vision request profile in successful coordinate/OCR details;
- end-to-end completion time;
- output length and end-to-end characters/second;
- image byte size and decoded dimensions for vision attempts.

Here, end-to-end means from entry into the shared Rust translation/vision
request through its final return. It includes provider payload preparation,
network time, provider-side work, streamed delivery, and production transport
retries. It excludes the preset wheel, fixture disk loading, screen capture,
benchmark pacing, and fallback to a different catalog model. Coordinate catalog
latency is the sum of its two shared model-call times; the deterministic local
crosshair crop between them is excluded as product setup rather than model
latency.

Text requests stream, matching the normal interactive text path.
Both coordinate calls and OCR requests are non-streaming, matching Rust
Computer Control and the built-in `Extract text` preset respectively. OCR
requests plain text.
Vision JSON is reserved for callers that supply an explicit response schema;
schema-less JSON would change model behavior without giving the caller a
parseable contract. These request semantics belong to the benchmark protocol,
so changing one requires a protocol-version bump.

For vision catalog timing, use the OCR representative cohort whose post-crop
input has a longest edge no greater than the policy's 1024 px boundary. This
matches normal small screenshot and crop interactions without throwing away
the large inputs: all ten OCR levels still count for accuracy and reliability,
and the all-case median/P95 remain stress diagnostics. Coordinate latency is the
sum of the locator and verifier provider-call times; benchmark pacing between
those calls is excluded. It stays available for control-specific decisions.
The manifest validator requires at least four representative cases in each
vision suite so a future fixture edit cannot silently collapse either timing
sample.

Use the selected latest run's full-result median for the catalog's user-facing
latency. The warm-only median, output length, provider retry evidence, and
full-result throughput may diagnose outliers, but no first-token proxy enters
benchmark reports or catalog decisions.
The same ten cases, exact source images, and round-major interleaving make model
comparisons fair. Within-run latency dispersion still combines task-size
sensitivity with provider variability; it is not a pure same-prompt load test.

Do not infer provider speed by comparing two endpoints that happen to share a
family name: provider, parameter count, quantization, image preprocessing, and
reasoning policy can all differ. Judge each provider-qualified catalog endpoint
from its own rows.

## Production vision timing logs

Every Windows vision-model call writes one copyable `[VisionPerf]` line to the
normal session log. It records the provider/model, source and wire image sizes,
image preparation time, provider-call start, observed transport retries and
wait, first real output, provider time, total time, output length, status, and a
bounded one-line error. It never logs the prompt text, image content, API key,
or model response.

After reproducing a slow real preset call, copy the latest traces with:

```powershell
Get-Content "$env:LOCALAPPDATA\SGT\logs\session.log" |
    Select-String '\[VisionPerf\]' |
    Select-Object -Last 50
```

For a non-streaming image preset, `first_output_ms` normally equals completion
time because the provider returns the answer as one response. Compare
`prepare_ms` with `provider_ms`: a large former value identifies local image
conversion, while a large latter value identifies provider/network queueing or
response work. Sequential error and success lines with different models expose
fallback delay directly.

Focused reruns can replace failed cells in an earlier report without repeating successful provider calls. Inputs are applied left-to-right, with the latest matching model/suite/case/round winning:

```powershell
$env:CATALOG_BENCH_MERGE_INPUTS = "target/catalog-benchmark/base/attempts.jsonl;target/catalog-benchmark/recovery/attempts.jsonl"
$env:CATALOG_BENCH_OUTPUT = "target/catalog-benchmark/complete"
cargo test catalog_benchmark_merge_reports -- --ignored --nocapture
```

The runner makes one provider request per text/OCR attempt and two per
coordinate attempt (locate, then verify). It adds no retry outside the shared
production transports. The pacer scopes independent model quotas by endpoint
and keeps OpenRouter provider-wide; deliberate pacing is not counted as model
latency. Errors—including
overload responses—are recorded so availability is part of consistency. The
production Gemini transport retries generic transient HTTP
429/500/502/503/504 responses at most twice with short jittered backoff. An
explicit provider retry delay above eight seconds fails fast so the
application's model fallback chain can advance. Groq vision retries one 429
only when its structural `retry-after` is at most two seconds; longer quota
windows fail immediately. Final provider errors retain the structured status,
message, retry delay, and quota metadata in `attempts.jsonl`.

The helper applies a 3.1-second minimum between OpenRouter requests even when
the general interval is lower. This stays below the no-billing account's
20-requests/minute gate, so benchmark reliability is not depressed by the
helper's own scheduling. A larger `CATALOG_BENCH_MIN_INTERVAL_MS` still wins.
This wait is excluded from measured model latency.

## Screen-text localization diagnostic

This non-history probe is deliberately bounded to three difficulty levels and
every enabled model in the default text-to-text priority stack. Each level may
contain multiple scripts and layouts. It uses Screen Translate's exact OCR-region
prompt, strict response schema, detector-owned ids, streaming transport, and
parser. Its latency is full-result completion like every other benchmark path.
The reviewed boxes remain attached only to ids, so the language model cannot
change geometry.

The report writes the normal `attempts.jsonl`, `summary.json`, and `summary.md`,
plus `localization-review.html` and paired PNG overlays. Green is reviewed
ground truth, cyan is the raw model box, and magenta is the bounded source-only
paint region. Raw-versus-painted IoU, source coverage, and excess overpaint are
retained per attempt.

```powershell
$env:CATALOG_BENCH_LOCALIZATION_PROBE = "1"
$env:CATALOG_BENCH_OUTPUT = "target/catalog-benchmark/localization-check"
cargo test catalog_benchmark_localization_probe -- --ignored --nocapture
```

Optional focused controls:

- `CATALOG_BENCH_LOCALIZATION_MODELS=id-a,id-b` selects text models; omission
  selects the entire default text-to-text priority stack.
- `CATALOG_BENCH_LOCALIZATION_LEVELS=1,2` selects a subset of the three levels.
- `CATALOG_BENCH_LOCALIZATION_CASES=localization-04-korean-player` selects exact
  case ids and can be combined with the level filter.

Because this is diagnostic evidence rather than a catalog ranking, its fixture
bytes and attempts are intentionally excluded from the catalog-history
fingerprint and latest-row selection.
