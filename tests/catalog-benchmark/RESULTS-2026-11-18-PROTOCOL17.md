# Benchmark day — 2026-11-18, protocol 17

The local Windows run recorded **918 merged cells across 34 provider-qualified
endpoints**. Providers returned 616 results. Codex directly reviewed all 368
successful cells requiring a rubric, with a verdict, a 1–5 rating, and a
judgment for each criterion. Review provenance is recorded as agent review.
The registered run is `import-20261117T1950210724974000000-06c1f7a6ba74`.

| Suite | Attempted | Returned | Direct reviews |
| --- | ---: | ---: | ---: |
| Text | 330 | 275 | 275 |
| OCR | 210 | 115 | — |
| Coordinates | 210 | 133 | — |
| Structured Screen Translate | 168 | 93 | 93 |

## Applied decisions

- Set Gemini 3.1 Flash Lite as the general image default and first image
  retry choice. It returned all ten OCR cases, with 0.928 mean character
  similarity and a 2,256 ms representative median. Gemini 3.5 Flash Lite
  remains the next choice. The default passes through the shared catalog to
  Windows and Android image presets, including Ask Image.
- Add OpenRouter `inclusionai/ling-3.2-flash:free` for text and vision.
  It returned all ten text and OCR cases, with text quality 4.70/5 and OCR
  similarity 0.936. It also produced parseable localization output on all six
  cases, with some mistranslation and a 24.6 s median on that diagnostic.
  Put it ahead of dots 3 Note in the text retry chain and use it after both
  Flash Lite rows in the image chain. Its catalog profile uses text-first
  images and the route's documented non-thinking control.
  [OpenRouter Ling 3.2 Flash](https://openrouter.ai/inclusionai/ling-3.2-flash:free)
- Move Qwen 3.8 to fourth for ordinary image retries. All four failed OCR
  cells hit receive deadlines again in focused recovery. A paced comparison
  reproduced a failure at the 3.0 s first-output deadline without hitting
  quota, then received the correct text through the relaxed stream. The
  second paced pair also succeeded at the interactive deadline. Keep Qwen
  as the text default. All ten text cases returned and reviewed at 4.80/5.
- Remove the Gemini 3.5, 3.6 and 3.8 Flash vision rows and the Gemini 3.6
  and 3.8 Flash text rows from shipped retry chains. They returned at most
  four results in their respective ordinary suites. Google reported high
  demand and exhausted resources, and multiple requests timed out. Keep
  the model IDs selectable and use stable Gemini 3.8 Live as the last
  image fallback. Gemini 3 Flash Preview returned four OCR results with
  strong successful-case similarity, so it is also outside the shipped
  image chain pending better interactive reliability.
- Disable OpenRouter's Gemma 4 26B text and vision rows after 30/30 base
  requests returned upstream 429s with daily account quota still available.
  Preserve their IDs and reviewed request policy for recovery and review.
- Refresh 21 existing latency values using rows with at least eight returned
  cases. OCR timing additionally requires at least four representative
  results. Keep sparse rows on their prior values. The signed NVIDIA feed
  continues to govern runtime endpoint offers. The Computer/Phone Control
  chain retains its existing acceptance and ordering.

## Selected base results

Times include full response completion. OCR medians use successful effective
images up to 1024 px on the longest edge. Similarity describes returned
transcriptions. Reliability includes failed cells.

| Model and suite | Returned | Reviewed text / OCR similarity | Catalog median |
| --- | ---: | ---: | ---: |
| Gemini 3.1 Flash Lite OCR | 10/10 | 0.928 | 2,256 ms |
| Gemini 3.5 Flash Lite OCR | 9/10 | 0.865 | 2,346 ms |
| Gemini Robotics-ER 2 OCR | 10/10 | 0.922 | 2,715 ms |
| Gemini 3.8 Live OCR | 10/10 | 0.882 | 4,134 ms |
| Ling 3.2 Flash OCR | 10/10 | 0.936 | 2,630 ms |
| Qwen 3.8 OCR | 6/10 | 0.988 | 1,952 ms, not applied |
| Qwen 3.8 text | 10/10 | 4.80/5 | 2,315 ms |
| Gemini 3.5 Flash Lite text | 10/10 | 4.80/5 | 1,333 ms |
| Ling 3.2 Flash text | 10/10 | 4.70/5 | 1,778 ms |
| dots 3 Note text | 10/10 | 4.50/5 | 2,837 ms |
| NVIDIA Nemotron 3.5 Lightning text | 10/10 | 4.10/5 | 2,168 ms |
| NVIDIA Laguna XS 2.1 text | 10/10 | 4.60/5 | 2,551 ms |
| Gemini 4 Hydrogen text | 10/10 | 5.00/5 | 29,329 ms |

## Candidate decisions and coverage limits

- Gemini 4 Hydrogen returned one OCR result and five strict coordinate
  successes. Five text answers took more than 40 s. High-demand errors,
  HTTP 429s and receive timeouts make it unsuitable for the current
  interactive catalog.
  The signed-in Project 1 table later confirmed a 20-request daily limit
  on the Free tier.
  Gemini 4 Argon and Carbon are paid-only on the documented standard tier.
  [Google pricing](https://aistudio.google.com/docs/pricing#gemini-4-hydrogen)
- Keep Google Gemma 4 26B vision and Gemini 2.5 Flash Lite disabled. They
  returned 4/10 and 2/10 OCR results respectively. The legacy 2.5 endpoint
  also returned project access 404s. Gemini 3.8 Live Extended Thinking
  returned ten text responses with 3.50/5 quality, including service-error
  prose and repeated reasoning. Its 12.95 s text median and 7.19 s
  representative OCR median offer limited benefit for the ordinary catalog.
- ERNIE and Flint each returned 10/10 ordinary text cases, at 5.00/5 and
  4.70/5. Both failed all six bounded structured calls. LFM3 returned 7/10
  text results and no usable structured results. Keep these routes out of
  the shared recommendations. North Mini Code returned 10/10 text with
  4.50/5 and a 563 ms median, but leaked reasoning and produced duplicated
  JSON in a structured response. Particle-1 returned 10/10 text at 4.90/5
  and 2,162 ms. Account quota blocked three structured cases and its vision
  grid, so defer admission.
- The run covered all six signed NVIDIA offers. Lightning again denied
  the 30-hour cancellation and returned source Japanese and Arabic in
  localization. Laguna XS 2.1 remains a feed offer with 4.60/5 text
  quality and a 2,551 ms median. The catalog imposes no new feed veto.
- OpenRouter's three counters finished with three aggregate free requests
  remaining. The discovery ledger identifies six candidates with incomplete
  or deferred modality coverage and records the route exclusions. The
  user later supplied the signed-in Google AI Studio table for Project 1.
  Its displayed daily limits agree with the corresponding catalog labels.
  Other Google projects remain unverified.
  Ordinary audio, sustained sessions, tool calling and end-to-end control
  tasks were outside the measured suites.

See [discovery and endpoint dispositions](DISCOVERY-2026-11-18.md).

## Request parity and evidence

The benchmark used the local Rust Windows build because the working tree's
Groq streaming and vision repetition behavior differed from the hosted
workflow revision. The runner called the shared text and vision entry
points with the same endpoint, reasoning, image part order, MIME handling,
output ceilings, schema policy, and provider retries as ordinary requests.
Text and coordinate calls used the documented 120 s benchmark budget. OCR
used the catalog-derived interactive deadlines. Uncataloged endpoints used
provisional profiles, and candidate OCR started with a 5,000 ms latency
seed. The applied Ling metadata uses its measured 2,630 ms median.
The benchmark exercised published or synthetic fixtures.

Gemini endpoint shards and focused recoveries were merged left-to-right
with the initial results. The individual attempts and original failures
remain under `target/catalog-benchmark/day-2026-11-18/`. The consolidated
`complete/` directory includes attempts, structured reviews, and run
metadata. The benchmark harness generated `target/catalog-benchmark/latest.json`
and `latest.md` from the registered run. OCR deadline diagnostics remain
outside the ranking and use their own JSONL files. `profile-evidence.json`
preserves the provisional request controls and manifest hashes.

Baseline catalog SHA-256:
`cbe647c17ce98edd3590a3991a9de191f789f872ff2411cd8b2857f9f626aaba`.
Applied catalog SHA-256:
`1e7fc89e46ea2901d4c1f8aec6e616951c7bb3628eaf8068943f011817ed8125`.

## Validation

- Windows managed `cargo check`, full `cargo test`, Clippy with warnings
  denied, formatting, and `git diff --check` passed. The full test used a
  separate output directory with the managed build cache and finished with
  3,920 unit tests and five integration tests passed, with 59 ignored.
- Android catalog validation and the preset test package passed for Full
  and Play. Each flavor ran 195 tests with one skipped. Both full flavor
  suites reported the same three failures in
  `HistoryMediaRemovalRaceTest`, which exercises file ownership during
  history-media deletion. The benchmark changes leave that subsystem
  outside their scope.
- All 23 newly applied timing rows match the registered latest report.
  Both catalog validators accept the retained Gemma vision request policy
  with the row disabled. The manifest check also rejects missing policies
  for enabled endpoints and policies for unknown endpoints.
