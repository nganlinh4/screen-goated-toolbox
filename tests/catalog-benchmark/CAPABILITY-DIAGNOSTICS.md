# Live capability diagnostics

These opt-in tests preserve evidence outside catalog history. They do not admit
models, change routing, execute pointer input, or replace the visible-runtime
acceptance contract in `docs/COMPUTER_CONTROL_DEVELOPMENT.md`.

Run each test binary from the repository root with a new absolute
`SGT_RUNTIME_STATE_ROOT`. Credentials use the benchmark's existing provider
pools. Copy a compiled test executable into the managed development evidence
cache before running it so another build does not attempt to replace a running
Windows executable.

## Ordinary production paths

`catalog_capability_probe` reads `CATALOG_CAPABILITY_PLAN`:

```json
{
  "models": [{"template_id": "<existing Live model ID>", "endpoint": "<exact endpoint>"}],
  "suites": ["text", "coordinate", "ocr"]
}
```

Every candidate requires an existing exact Live endpoint profile. Set
`CATALOG_CAPABILITY_OUTPUT` to a new report directory. All ten cases use the
ordinary production request path and its endpoint reasoning settings. Template
presentation metadata is not a candidate capability or performance claim.

## Native tool grounding

`native_live_grounding_probe` reads `LIVE_GROUNDING_PLAN`:

```json
{"models": ["<exact endpoint>"], "repetitions": 2}
```

Set `LIVE_GROUNDING_OUTPUT` to a new JSONL file. The test uses the shared Live
setup/transport/parser, HIGH media resolution, a 1536-pixel maximum image edge,
and LOW thinking when configurable. A dedicated reporting function returns a
point without executing an action. Interaction-idle profiles use non-blocking
functions; other profiles use blocking functions. It grades against the same
reviewed coordinate boxes and records completion, tool calls, transcript and
wire dimensions. Locator hits are not verified control-task success.

Optional `ws_base` compares documented API versions. `cases` selects exact
coordinate case IDs; `frame_count` (1–4) repeats the frame at one frame per
second. Optional `speech` maps each selected case ID to a public generated
mono PCM16/16kHz WAV. Spoken-input runs use explicit activity boundaries and
record input transcription; text runs retain their original input path.
Wire diagnostics omit audio payloads and session-resumption handles.
Optional `thinking_level` selects `LOW`, `MEDIUM`, or `HIGH` for configurable
endpoints; it defaults to `LOW`. Optional `deadline_seconds` selects a bounded
completion deadline of 1–180 seconds (default 45). Keep separate evidence for
each configuration; general catalog speed settings do not constrain a control
feature's quality-first evaluation.

## Full-catalog tool sequencing

`control_provider_capability_probe` reads `CONTROL_CAPABILITY_PLAN`:

```json
{
  "models": ["<exact endpoint>"],
  "cases": [{"id": "<case>", "prompt": "<natural read-only goal>",
    "files": ["<absolute fixture file>"], "delay_ms": 3000}]
}
```

Set `CONTROL_CAPABILITY_OUTPUT` to a new JSONL file. It acquires the verified
production control engine and builds the full production prompt/tool catalog.
The diagnostic selects endpoint-compatible reasoning and tool behavior. Only
reads of the listed canonical fixture paths reach the production file reader;
every other requested operation receives a diagnostic-boundary result.
Delaying receipts tests whether reasoning resumes after external work. Review
the final answer independently against the fixture contents, including missing
facts, incorrect arithmetic, premature completion, and post-completion tools.
This tests provider integration, not desktop/phone action acceptance.

The control plan also accepts `ws_base` and `thinking_level` (`LOW`, `MEDIUM`, or
`HIGH`, default `LOW`). A test-process success means evidence
collection completed, not that a candidate passed. Inspect each record's
completion, answer, calls and errors before deciding model suitability.
