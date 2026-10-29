# Benchmark day — 2026-10-29, protocol 16

Completed 696 merged cells across 26 provider-qualified endpoints: the hosted enabled-catalog and signed-feed grid (workflow run 40862995538) plus one isolated OpenRouter candidate. 542 cells returned successfully. All 312 successful manual-review cells carry a verdict, 1–5 rating and authored-rubric checks: 117 reuse a 2026-10-18 judgment of byte-identical response, reference and rubric, and 195 were judged directly. These are **agent reviews (Claude), not human sign-off**.

The consolidated registered run is `import-20261029T1214209764978000000-8f27efffbf3d`. It uses the unchanged protocol-16 fixtures and request semantics, so no protocol bump was needed.

## Applied decisions

- Keep every generic head and leading fallback. Groq Qwen 3.8 returned 10/10 text (4.70/5, 285 ms) and 10/10 OCR (0.972 aid, 1,471 ms representative). Gemini 3.5 Flash Lite remains the immediate image fallback (9/10 OCR, 964 ms).
- Move Gemini 3.8 Flash to the tail of both generic chains. The complete run returned 5/10 text, 0/10 OCR and 2/10 coordinate cells, with receive-body timeouts and HTTP 503 "high demand" errors. A diagnostic rerun from a different host about an hour later returned 5/10 text, with the slowest successes taking 97–116 s, and 2/10 OCR. The answers it did return reviewed at 5.0/5, so this is a reliability demotion, not a quality finding. dots 3 Note now precedes it for text, and 3.1 Flash Lite precedes it for images. Gemini 3.7 Flash, which Google routes to 3.8 Flash, failed the same way (4/10 text, 1/10 OCR). It is not in either chain.
- Withdraw OpenRouter `nvidia/nemotron-3-super-120b-a12b:free`. The provider removed the free route. Discovery no longer lists it as zero-priced, and every request returns HTTP 404 "This model is unavailable for free", naming the paid slug instead. Paid-only endpoints are excluded, so the row and its profile are removed and `withdrawn_models` prevents reintroduction. It was not in any chain. The NVIDIA-hosted Nemotron 3 Super row is a separate endpoint and is unaffected.
- Refresh 28 latency values from decision-ready rows with at least 8/10 successful cells. Three rows keep their earlier values because their medians come from half-failed or nearly empty runs: 3.7 Flash text (4/10), 3.8 Flash text (5/10) and OpenRouter Gemma 4 26B text (1/10). Unready vision rows are also unchanged. Intelligence tiers are unchanged.
- Interactive deadlines derive from catalog latency. Qwen vision's first-attempt budget therefore grows from the 5 s floor to 5,884 ms (4 × 1,471 ms).
- Do not add OpenRouter `recursive/flint-v1.0:free`, the only new free general endpoint. It returned 10/10 text at 4.70/5, but with a 2,154 ms median and a 23 s tail. It failed all 6 structured Screen Translate cells with "Provider completion token limit reached", because its reasoning is mandatory and consumes the bounded output. It is text-only and offers no quality, latency, quota or modality advantage over dots 3 Note.
- Keep Google Gemma 4 26B at text position 7. Today's row (8/10, two HTTP 500s, 2,050 ms) follows a 10/10, 1,017 ms row on Oct 18. Its 14,400 requests/day quota still outranks the 20-RPD Gemini rows behind it.

Generic preset heads, explicit-search and audio defaults, provider recommendations and the Computer/Phone Control chain are unchanged. Windows adopts the changed chains through the existing post-update prompt. Persisted user chains are not overwritten.

## Core evidence

Times are full-result completion medians. OCR catalog times use successful inputs whose effective longest edge is at most 1024 px. Quality ratings come from direct review of this small suite and are not a universal intelligence measure.

| Model row | Suite | Returned | Reviewed quality / OCR aid | Catalog median ms |
| --- | --- | ---: | ---: | ---: |
| `groq-qwen-3-8-27b-text` | text | 10/10 | 4.70/5 | 285 |
| `groq-qwen-3-8-27b-vision` | ocr | 10/10 | 0.972 | 1,471 |
| `groq-gpt-oss-20b-text` | text | 10/10 | 4.50/5 | 332 |
| `groq-gpt-oss-120b-text` | text | 10/10 | 4.50/5 | 379 |
| `google-gemini-3-5-flash-lite-text` | text | 10/10 | 4.90/5 | 642 |
| `google-gemini-3-5-flash-lite-vision` | ocr | 9/10 | 0.921 | 964 |
| `google-gemini-robotics-er-2-text` | text | 10/10 | 4.90/5 | 572 |
| `google-gemini-robotics-er-2-vision` | ocr | 10/10 | 0.911 | 806 |
| `google-gemini-3-flash-text` | text | 10/10 | 4.90/5 | 883 |
| `google-gemini-3-flash-vision` | ocr | 10/10 | 0.940 | 1,445 |
| `google-gemini-3-6-flash-text` | text | 10/10 | 4.80/5 | 924 |
| `google-gemini-3-1-flash-lite-text` | text | 10/10 | 4.90/5 | 1,974 |
| `google-gemini-3-1-flash-lite-vision` | ocr | 8/10 | 0.995 | 1,747 |
| `google-gemini-3-8-live-text` | text | 10/10 | 4.70/5 | 1,849 |
| `google-gemini-3-8-flash-text` | text | 5/10 | 5.00/5 (n=5) | 41,705 (not applied) |
| `google-gemini-3-8-flash-vision` | ocr | 0/10 | — | — |
| `openrouter-dots-3-note-text` | text | 10/10 | 4.60/5 | 1,625 |
| `openrouter-recursive-flint-v1-0-text` | text | 10/10 | 4.70/5 | 2,154 (candidate) |
| `nvidia-nemotron-3-5-lightning-text` | text | 10/10 | 3.70/5 | 379 |
| `taalas-llama-3-1-8b-text` | text | 10/10 | 3.10/5 | 111 |

Structured Screen Translate diagnostics (not catalog history):
- Qwen 3.8 reviewed 3.67/5.
- dots 3 Note reviewed 3.50/5. It made translation errors in all three CJK and Arabic cases.
- Gemini 3.5 Flash again reassigned the Arabic translations to the wrong detector ids. Gemini 3.6 Flash did not repeat that this time.
- NVIDIA MiniMax M3.1 Flash and Nemotron 3.5 Bolt scrambled the ids of the 13-region comment thread.

## Open recommendation (not applied)

NVIDIA Nemotron 3.5 Lightning is offered by the signed feed and enters adaptive ordering after the four authored text leaders. It reviewed at 3.70/5 on text: it computed a 6-hour interval and denied the refund in text 08, and invented a $9,890 total in text 10. On structured Screen Translate it reviewed at 1.83/5: it returned Japanese, and 15 of 16 Arabic regions, untranslated. Its 379 ms latency can rank it ahead of stronger Gemini fallbacks.

A provider-qualified veto would be a quality judgment, not the output-contract finding that justified earlier vetoes. It is therefore left for an explicit decision. Withdrawn feed rows stay excluded on both platforms.

## Discovery and coverage

See [DISCOVERY-2026-10-29.md](DISCOVERY-2026-10-29.md) for the endpoint ledger.

**Gemini.** 20/20 credentials returned one identical inventory, fingerprint `sha256:68364a87…`. Compared with Oct 18 there are no new endpoints. Three Veo video-generation endpoints were removed; they were never cataloged.

**Groq.** From hosted runners, 2/2 credentials returned an inventory identical to Oct 18; `allam-2-7b` remains API-visible but absent from the Free Plan table. This PC currently receives a network-level HTTP 403 "Access denied. Please check your network settings" from both the API and the docs site, with or without a credential. That makes it a local-network block, not a key revocation. Local discovery therefore failed for Groq, and the hosted evidence is authoritative.

**OpenRouter.** 19 zero-priced entries:
- one new route, Flint, benchmarked above;
- one removal, Nemotron 3 Super free, withdrawn above;
- the same exclusions as Oct 18: an unpinned router, content safety, and Lyria music generation.

The daily counters ended at 15/50, 15/50 and 22/50. The 11 routes rejected on Oct 18 were not retested. Remaining quota could not cover their full grids, and their Oct 18 dispositions stand.

**NVIDIA.** The signed feed offered five endpoints, and all ran with their feed controls: MiniMax M3.1 Flash, Nemotron 3.5 Lightning, Nemotron 3.5 Bolt, Nemotron 3.5 Super VL and Nemotron 3 Ultra. The catalog's NVIDIA GPT-OSS 20B/120B and Nemotron 3 Super rows were not offered, so their historical values are unchanged. No feed candidate is promoted statically from this run.

**Untested here.** Audio, native tool calling, sustained sessions, dedicated generation and full control-task acceptance remain untested. Coordinate rows are control-task evidence only; they do not change the control chain.

## Production parity

The hosted grid ran through the shared `translate_text_streaming` and image production entry points. These preserve provider URLs, part order, MIME and resolution profiles, streaming mode, native output ceilings, schema handling, parsing, retries and the catalog/feed reasoning controls; ordinary calls disable search.

The Flint candidate and the 3.8 Flash diagnostic ran locally from an isolated checkout of the same commit (`d2106fb0`), using a test-only candidate profile. The application catalog was never modified by candidate preparation. Flint's provisional profile copied OpenRouter defaults with reasoning `not-applicable`, because its reasoning is mandatory and exposes no disable control. Candidate and baseline hosts differ, so small cross-host latency differences are not a controlled comparison.

## Evidence and validation

Local evidence (Git-ignored) is under `target/catalog-benchmark/day-2026-10-29/`:
- `complete/`: the merged attempts and `reviews.json` with reviewer provenance;
- `hosted/`: the workflow artifacts;
- `candidates/`;
- `diagnostics/`;
- `discovery/`, with quota snapshots;
- `review-packets/` and `review-decisions/`;
- `reproduction/`: the scripts, logs and candidate binary hash.

Credentials are excluded.

The applied catalog SHA-256 is `469f4de74d6127714b165b61fadbe465d06594093edb99bed7fc335d9ba79486`. These checks passed after the changes:
- manifest validation;
- managed `cargo check`;
- `cargo test` (2,032 passed, 47 ignored);
- `cargo clippy --all-targets -- -D warnings`;
- `cargo fmt -- --check`;
- `git diff --check`;
- Android unit tests: 1,144 Full and 169 focused Play preset/model/catalog tests, with no failures.
