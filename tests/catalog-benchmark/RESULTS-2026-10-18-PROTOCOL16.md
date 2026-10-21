# Benchmark day — 2026-10-18, protocol 16

Completed 1,118 merged cells across 41 provider-qualified endpoints: 380 text, 270 coordinate, 270 OCR and 198 structured screen-translation diagnostics. The ordinary grid contains 92 complete ten-case model/suite rows. 807 cells returned successfully. All 441 successful manual-review cells received direct Codex verdicts, 1–5 ratings and authored-rubric checks; these are **agent reviews, not human sign-off**.

Benchmark collection initially left the application unchanged. The user subsequently authorized applying the reviewed results. The shared catalog now contains the ordering, vision eligibility, latency and withdrawal decisions below. Previously dirty creation-feature files were preserved. Provisional test profiles remain outside the application.

The history rules mark 71 ordinary rows decision-ready; 21 retain explicit access, quota, contract or representative-sample gaps. Ten attempted cells do not imply ten usable answers. Every selected row comes from the consolidated run.

## Applied decisions

- Keep direct Groq Qwen 3.8 as the general text and OCR head. It remains fast and strong on these tasks. Its coordinate strict pass rate is zero, so this does not endorse it for Computer/Phone Control.
- Keep Gemini 3.5 Flash Lite as the immediate image fallback and promote it from text position 8 to position 4, ahead of 3.1 Flash Lite. Both text rows returned 10/10 at reviewed 4.9/5; their full-result medians were 543 ms and 3,250 ms respectively. This keeps a Google fallback in the four authored text leaders protected from availability-feed interleaving.
- Refresh 34 enabled text/OCR latency values from the selected decision-ready rows. Leave audio curation, unready vision rows and feed-withheld endpoints' historical values unchanged. Retain existing intelligence tiers: this task suite does not establish a new six-tier calibration, and automatic OCR similarity is not an intelligence rating.
- Keep the first three Groq text rows. In the remaining text stack, use canonical Gemini 3.6 Flash instead of its 3.5 alias; 3.6 returned 10/10 at 5.0/5 and 1,188 ms. Move Gemini 3.8 Live (10/10, 4.7/5, 1,922 ms) ahead of 3.8 Flash (9/10, 4.89/5, 6,138 ms). Replace the final OpenRouter Nemotron row (8/10, 4.5/5) with the already admitted dots 3 Note (10/10, 4.6/5). The OpenRouter allowance remains account-wide; this is provider diversity, not extra quota per model.
- Disable only Google Gemma 4 26B's ordinary vision row after all ten OCR calls timed out receiving the body. Remove its ordinary vision request profile and image-chain position; keep its successful text row and shared endpoint profile. This is an interactive transport/deadline finding, not a claim that the endpoint cannot see images. The image chain now contains nine supported defaults, rather than adding an unsupported replacement to reach ten.
- Move Gemini 3.1 Flash Lite to the image-chain tail: six OCR answers returned, with four receive-body timeouts. Its 0.998 similarity describes only returned answers. Preserve Qwen and 3.5 Flash Lite as the image leaders and retain the other established image fallbacks.
- Add no new model. Legacy Gemini access is limited to a few credentials, Live Extended Thinking still breaks ordinary output contracts, and new OpenRouter routes offer no demonstrated reliability/quality/quota advantage that warrants admission.
- Withdraw NVIDIA `stepfun-ai/step-5-preview` from ordinary routing. All ten raw ordinary responses contain analysis and `</think>` alongside the answer, despite the signed feed selecting `effort-none`. The provider-qualified catalog veto prevents a signed offer from reintroducing that current ordinary profile on either platform. This is an SGT endpoint/profile output-contract finding; native tools, vision and other configurations remain untested.
- Preserve signed NVIDIA feed governance for existing dynamic availability. Do not turn its short availability-probe latency into catalog quality, or statically promote its candidates from this run.

DiffusionGemma remains feed-selectable and was not withdrawn. Direct OCR inspection found a missing newspaper headline, invented rule-line characters, altered filenames and a changed byte count, alongside exact compact-label, multilingual-sign and product/price answers. Those specific errors support retaining the established leaders; the 0.895 similarity aid alone does not decide admission or order. Its 970 ms representative median and Qwen's 974 ms do not establish a meaningful speed difference, especially across hosts.

Generic preset heads, explicit-search and audio defaults, provider recommendations and the separate Computer/Phone Control chain remain unchanged. Windows adopts changed recommendations through the existing post-update prompt; Android clean/reset defaults use the generated catalog. Persisted user chains are not silently overwritten. No application release or online-feed publication is part of this change.

## Core evidence

Times are full-result completion medians. OCR catalog times use only successful inputs whose effective longest edge is at most 1024 px, with at least four representative samples. Failure times are excluded from latency samples and retained in reliability. Ratings describe this small task suite, not universal intelligence.

The enabled-catalog baseline ran on hosted Windows runners; additional candidates ran locally in an isolated checkout with the same source, fixtures and production paths. This is not a controlled same-region latency A/B. Small differences across those hosts do not establish a speed advantage. Applied ordering uses the established baseline rows and documented reliability/output-contract findings, without promoting local candidates from small timing differences.

| Model row | Suite | Returned | Direct Codex quality / OCR aid | Catalog median ms | Latency n |
| --- | --- | ---: | ---: | ---: | ---: |
| `groq-qwen-3-8-27b-text` | text | 10/10 | 4.90/5 | 246 | 10 |
| `groq-qwen-3-8-27b-vision` | ocr | 9/10 | 0.962 | 974 | 7 |
| `groq-gpt-oss-120b-text` | text | 10/10 | 4.80/5 | 442 | 10 |
| `groq-gpt-oss-20b-text` | text | 10/10 | 4.30/5 | 310 | 10 |
| `google-gemini-3-5-flash-lite-text` | text | 10/10 | 4.90/5 | 542 | 10 |
| `google-gemini-3-5-flash-lite-vision` | ocr | 10/10 | 0.929 | 1,344 | 7 |
| `google-gemini-3-1-flash-lite-text` | text | 10/10 | 4.90/5 | 3,250 | 10 |
| `google-gemini-3-flash-text` | text | 10/10 | 5.00/5 | 894 | 10 |
| `google-gemini-3-6-flash-text` | text | 10/10 | 5.00/5 | 1,188 | 10 |
| `google-gemma-4-31b-text` | text | 8/10 | 4.88/5 | 16,846 | 8 |
| `google-gemini-3-8-live-extended-text` | text | 9/10 | 3.89/5 | 8,060 | 9 |
| `nvidia-stepfun-ai-step-5-preview-fee2f8ae` | text | 10/10 | 1.90/5 | 3,618 | 10 |
| `nvidia-google-diffusiongemma-26b-a4b-it-cc557372` | ocr | 10/10 | 0.895 | 970 | 7 |
| `openrouter-dots-3-note-text` | text | 10/10 | 4.60/5 | 1,488 | 10 |
| `taalas-llama-3-1-8b-text` | text | 10/10 | 3.00/5 | 44 | 10 |

OCR similarity is an automatic aid. Coordinate returned counts mean locator/verifier output was parseable, not that the location was correct. The separate strict outcome requires a target hit and the fresh-crosshair verification gate. No whole control task, verifier safety replacement or native tool chain was accepted by these static fixtures.

## New and previously rejected endpoints

Gemini 2.5 Flash Lite completed 10/10 text, coordinate and OCR cells after restricting recovery to the three credentials that actually have legacy access. Seventeen others return lifecycle 404s. It remains disabled globally: a successful eligible pool does not establish access for new projects, and its primary-project allowance is only 20 RPD. Gemini 2.5 Flash has only two eligible credentials and reaches its 20 RPD per-project ceiling before full quality coverage. Gemini 2.5 Pro has no runnable pool: 19 credentials return lifecycle 404 and one has free RPM/RPD/TPM of zero.

Live 3.8 Extended Thinking was retested with its exact existing low-thinking and interaction-idle profile. It still returns unwanted prefaces, collapsed output lines and a completed system-error message in place of the Chinese translation. It is not recommended for ordinary tasks.

| New OpenRouter endpoint | Text returned | Reviewed quality (successful responses only) | Coordinate returned | OCR returned | Decision |
| --- | ---: | ---: | ---: | ---: | --- |
| `xingchen-agi/xing4.0-29b-a4b:free` | 10/10 | 3.20/5 (n=10) | not applicable | not applicable | Do not add; see exact access/quota/output gaps in ledger. |
| `nvidia/nemotron-3.5-super-vl:free` | 9/10 | 4.44/5 (n=9) | 9/10 | 6/10 | Do not add; see exact access/quota/output gaps in ledger. |
| `liquid/lfm-2.5-2.6b:free` | 10/10 | 4.30/5 (n=10) | not applicable | not applicable | Do not add; see exact access/quota/output gaps in ledger. |
| `nvidia/nemotron-3.5-lightning:free` | 9/10 | 3.56/5 (n=9) | not applicable | not applicable | Do not add; see exact access/quota/output gaps in ledger. |
| `thinkingmachines/inkling-small:free` | 0/10 | — (n=0) | 0/10 | 0/10 | Do not add; see exact access/quota/output gaps in ledger. |
| `poolside/laguna-s-2.1:free` | 10/10 | 4.60/5 (n=10) | not applicable | not applicable | Do not add; see exact access/quota/output gaps in ledger. |
| `thinkingmachines/inkling:free` | 0/10 | — (n=0) | 0/10 | 0/10 | Do not add; see exact access/quota/output gaps in ledger. |
| `poolside/laguna-xs-2.1:free` | 4/10 | 4.75/5 (n=4) | not applicable | not applicable | Do not add; see exact access/quota/output gaps in ledger. |
| `cohere/north-mini-code:free` | 10/10 | 4.20/5 (n=10) | not applicable | not applicable | Do not add; see exact access/quota/output gaps in ledger. |
| `nvidia/nemotron-3-ultra-550b-a55b:free` | 1/10 | 5.00/5 (n=1) | not applicable | not applicable | Do not add; see exact access/quota/output gaps in ledger. |
| `nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free` | 4/10 | 5.00/5 (n=4) | 0/10 | 8/10 | Do not add; see exact access/quota/output gaps in ledger. |
| `google/gemma-4-31b-it:free` | 0/10 | — (n=0) | 0/10 | 0/10 | Do not add; see exact access/quota/output gaps in ledger. |

Every current eligible OpenRouter route was attempted across the applicable ten-case grid. That does not imply complete quality coverage: successful easy cases cannot substitute for missing hard cases. Preserve every route-qualified failure and defer unsupported semantic conclusions. `INVALID_API_KEY` on Inkling is route-specific access evidence; the same credential pool works on other routes. Google Gemma free-route upstream 429s are distinct from the account’s later daily ceiling.

## Quota and lifecycle

The run includes one call per ordinary text/OCR cell and a locator plus verifier for coordinates when the locator succeeds, plus six structured diagnostics per supported text endpoint. Production retries consume additional provider quota. These cells are not a claim of one request per coordinate task.

OpenRouter’s authenticated key endpoint reports `free_model_daily_requests`, with a limit of 50 on each observed key counter. Different observed counters do not prove that arbitrary extra keys multiply one account’s allowance. Baseline and candidate calls were serialized provider-wide at the existing minimum interval; quota snapshots and final structural errors are preserved. No credits were purchased.

Focused Poolside and Xing recoveries used the key with verified reported remaining capacity. The final key counters report 46/50, 48/50 and 50/50 used. Those remaining counts are observations, not a guarantee of independent account allowance. Other unresolved cells retain their exact upstream, quota or production-contract blocker; no admission depends on completing the failed supplemental Lightning probes.

Groq Qwen’s baseline structured probe hit an explicit 1,000 output-token-per-minute organization limit. A focused same-profile recovery ran only that failed cell after the minute window cleared and returned successfully. The original quota failure remains in its fragment; this is not evidence of a language failure or a claim that extra keys create independent organization quota.

Google’s [current lifecycle page](https://ai.google.dev/gemini-api/docs/deprecations) restricts 2.5 access to prior active users. It also says 3.5 Flash routes to 3.6 Flash and 3.7 Flash routes to 3.8 Flash; these are not independent fallback models. Live 2.5 and Live 3.1 have a November 17, 2026 shutdown date. API listing alone does not reverse a shutdown or establish a free tier.

The pricing parser fuzzy-matches 3.5 Flash to the Flash-Lite heading and misses some exact headings. The ledger corrects those observations against [official pricing](https://ai.google.dev/gemini-api/docs/pricing), lifecycle records and authenticated project limits. Omni 1.1 is paid video generation. Image/music/video, speech, embedding, safety, managed-agent and unpinned router contracts remain outside ordinary inference. Antigravity and grounding integration were not reopened.

Two listed session endpoints remain explicit contract gaps: `gemini-2.5-flash-native-audio-preview-09-2025` and `gemini-robotics-er-2-streaming-preview` lack exact compatible SGT session profiles. Groq `allam-2-7b` is visible in the API but absent from its official Free Plan table; free eligibility remains unverified. These are not silently counted as successfully benchmarked general endpoints.

## Production parity and review findings

The ignored Rust runner calls the shared `translate_text_streaming` and image production entry points. It preserves provider URLs, image part order, MIME and resolution profiles, streaming mode, native output ceilings, schema handling, parsing, repetition protection, retries and reasoning controls. Ordinary calls disable search. OpenRouter candidates use its nested reasoning object and exact live metadata: optional routes use supported `none`; mandatory LFM reasoning omits an unsupported override. NVIDIA uses the verified signed feed’s typed control. No HTTP timing script replaces those paths.

- Both coordinate stages use the real point/schema and fresh-crosshair path. Wire sizes, image preparation and per-model deadlines are retained. Slow or failed OCR under interactive deadlines remains production reliability evidence.
- Qwen structured text can lose the final JSON brace while streaming recovery still yields all regions. Direct review marks strict raw-schema compliance false; successful recovery is not a strict-schema pass.
- Gemini 3.5/3.6 structured Arabic output reassigns translations to the wrong detector IDs. All IDs may exist while the translated screen is still semantically wrong. Both source-to-ID mapping and content were reviewed.
- Taalas is very fast but fails classification, multilingual fidelity and synthesis. Speed alone does not support a recommended-chain position.
- All successful text and structured outputs were read directly. Identical exact response/reference/rubric content shares one explicit judgment, expanded back into every cell. Quoted UI-label preservation in text case 09 is ambiguous against the reference, so either style is accepted when narrative translation is correct. Text case 10 has an underspecified comparable-quote condition; factual prerequisites, arithmetic and formatting are evaluated explicitly, rather than failing solely on reference wording.

The purchase policy separates approval from service start. Explicit signed-DPA-before-start gates receive credit even with APPROVE wording; a word alone does not decide the verdict. The fixture could state customer-data handling and quote comparability more clearly. Clear Blocker None/proceed replies still fail its authored prerequisite rubric. No fixture was changed.

All coordinate/OCR source images and target boxes were visually checked through local contact sheets and original crops. No fixture was modified. Audio quality, native tool calling, sustained sessions, dedicated generation and full control-task acceptance remain untested.

## Evidence and validation

The applied catalog SHA-256 is `52357699aa555e20565f351bd0f9a9369bd625da92537bbf1187f80bd733f40d`.
The subsequent application passed manifest validation, managed `cargo check`,
all 2,028 Rust tests (45 ignored), and `cargo clippy --all-targets -- -D warnings`.
Changed Rust files pass formatting checks, and `git diff --check` passes.
Android regeneration/compilation and 124 focused preset/model/presentation tests
passed in each of the Full and Play flavors, including signed-feed withdrawal
and disabled-vision availability checks. Existing recommendation-adoption tests
passed on Windows; both platforms use the same updated retry fixture.

The full Android suites were also run: Full had 1,144 tests and Play had 1,114,
with two unrelated creation-feature failures in each. `CreationParityContractTest`
and `ImageCreatorReleaseAvailabilityTest` still assert that image creation is
hidden, while the pre-existing dirty creation fixture enables it. Those files
were preserved. Full-repository Rust formatting also retains the two existing
unchanged-file differences listed below. Neither check is reported as clean.

The consolidated registered run is `import-20261018T0207144042302000000-052080fc1638`. [Hosted workflow](https://github.com/nganlinh4/screen-goated-toolbox/actions/runs/39085676318) completed successfully. Baseline fragments, isolated candidates and focused recoveries were merged left-to-right; successful cells were not repeated during recovery. Earlier raw access errors and all fragments remain preserved. History selects the newest complete compatible row; it does not average old runs.

- [Complete raw attempts, reviews and generated summary](../../target/catalog-benchmark/day-2026-10-18/complete/summary.md).
- [Portable evidence archive](../../target/catalog-benchmark/day-2026-10-18/benchmark-evidence-2026-10-18.zip) and [selected history](../../target/catalog-benchmark/latest.md).
- [Full discovery ledger](DISCOVERY-2026-10-18.md), [machine-readable ledger](../../target/catalog-benchmark/day-2026-10-18/discovery-ledger.json), [coverage](../../target/catalog-benchmark/day-2026-10-18/coverage.json), [exact recovery list](../../target/catalog-benchmark/day-2026-10-18/missing-case-recovery.json), and [completion receipt](../../target/catalog-benchmark/day-2026-10-18/completion-receipt.json).
- Catalog manifest validation and fixture validation passed. Managed `cargo check`, full `cargo test` (2,028 passed, 45 ignored), and `cargo clippy --all-targets -- -D warnings` passed.
- `cargo fmt -- --check` reports existing formatting differences in unchanged `src/overlay/three_d_generator/ipc.rs` and `src/overlay/three_d_generator/runtime/capabilities.rs`; they were not altered for benchmark work. `git diff --check` passed.
- Benchmark input catalog SHA-256 was `ec7fff68e727d06461bf63cbd1971dce96f3df47cc9404d822dccedfec8f439c`. The subsequent catalog application is recorded separately; raw benchmark receipts remain immutable. The isolated candidate catalog, executable hash, source commit and run receipts are retained with the evidence. No application release, production promotion or runtime upload occurred.
- Automatic approval review blocked the final temporary-file cleanup command before execution. The isolated checkout, copied test executable and isolated runtime remain in the managed cache; benchmark evidence and application state are unaffected.
