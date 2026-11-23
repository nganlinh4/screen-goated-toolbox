# Benchmark day — 2026-11-23, protocol 17

[Discovery and route eligibility](DISCOVERY-2026-11-23.md)

## Catalog decisions

- Restore Groq Qwen 3.8 as the generic image default and first image retry.
  It returned all ten OCR cases with 0.973 mean character similarity and an
  812 ms median across seven representative inputs. Direct inspection found
  mostly punctuation, form-fill and proper-name errors. Gemini 3.5 Flash Lite
  and Gemini 3 Flash Preview returned 10/10 after same-policy recovery and
  follow Qwen. Gemini 3.1 Flash Lite returned 8/10 and remains selectable and
  in the chain. Ling 3.2 Flash moves behind the Gemini fallbacks after upstream
  errors reduced its baseline to two completed OCR cases.
- Keep Qwen 3.8 as the text default. It completed all ten levels with a 495 ms
  full-result median and a 4.70/5 agent review. Move GPT-OSS 120B ahead of 20B.
  Both completed ten levels, but 120B scored 4.70/5 against 4.10/5 with only
  62 ms additional median latency. The 20B output approved a request with an
  unsigned DPA in the final synthesis case.
- Admit OpenRouter Particle-1 for ordinary text. It returned all ten cases
  at a 760 ms median with a 4.80/5 direct review. It precedes Ling 3.2 Flash
  among OpenRouter text fallbacks. Vision remains provisional after repeated
  upstream errors. [Particle-1 route](https://openrouter.ai/particle-ai/particle-1:free)
- Disable the Dots 3 Note free text and vision rows. OpenRouter's exact route
  reported no serving endpoints. Preserve the catalog IDs and request profiles
  and omit both rows from the shipped chains.
  [Exact route availability](https://openrouter.ai/api/v1/models/dots-studio/dots-3-note-preview:free/endpoints)
- Refresh latency from complete rows with eight or more returned answers.
  Text rows require direct structured review and OCR rows require at least
  four representative inputs. Coordinate results inform the separate control
  evaluation. This run changes the general image and text recommendations.

## Request and review evidence

Local Windows harnesses called the application's Rust text and vision entry
points. The baseline used the working catalog, and the provisional profiles
were built from an isolated source copy. The exact provisional catalog and
binary hashes are saved with the dated evidence. The harness used native
reasoning controls, request profiles, schema handling and production retries.
Text and coordinate calls used the 120 s benchmark budget. OCR used the
catalog-derived interactive deadlines. The coordinate locator and verifier
were measured separately from ordinary OCR, and localization diagnostics
remain outside the catalog ranking.

Gemini, Gemini Live and Groq used endpoint shards. OpenRouter used one
provider-wide shard at a time. NVIDIA used the signed feed's five offers.
Focused OCR recoveries skipped successful cells. The dated evidence retains
the original attempts and each recovery alongside the consolidated report.

## Candidate decisions and recovery gaps

- Gemini 4 Hydrogen's `LOW` profile returned 7/10 text, 2/10 OCR and 4/10
  coordinate responses. Google reported high demand and OCR receive timeouts.
  Successful text had a 72.1 s median. Defer admission while the ordinary
  interactive profile has sparse vision evidence and long text completion.
- Gemini 3.8 Live Extended Thinking returned 10/10 OCR at a 2.79 s small-image
  median and a 21.7 s representative P95. The fixed 5 s provisional latency
  used for deriving deadlines makes that tail unsuitable for promotion on the
  measured median without another deadline check. Text scored 3.60/5 and
  included service-error transcripts. Keep the current stable Live rows.
- Gemini 2.5 Flash Lite remains disabled after project-access errors. Gemini
  Gemma 4 26B vision remains disabled after sparse returns and provider
  errors. Gemini 3.5, 3.6, 3.7 and 3.8 Flash and Gemma 4 31B crossed the
  OCR deadline or reported overload or quota failures on most cases.
- OpenRouter Particle-1 vision returned 5/10 OCR and 5/10 coordinate
  responses. The two Gemma free routes returned zero ordinary responses.
  Ling 3.2 Flash's early 429s were upstream errors while the account counters
  still had requests. Later, the candidate shard exhausted all three free
  daily allowances, limiting recovery and supplemental localization.
- OpenRouter ERNIE 5.5 Flash and Liquid LFM 3 returned 8/10 and 9/10 text
  answers, respectively, behind Particle-1 on observed reliability and
  median latency. Flint returned 10/10 at 3.58 s median with a 69.9 s final
  synthesis. Keep them outside shipped recommendations. The OpenRouter
  NVIDIA text routes returned all ten answers, but their reviewed quality
  trailed Particle-1, with either wrong policy decisions or formatting loss.
- OpenRouter Nemotron 3.5 Super VL and Nano Omni returned 10/10 OCR each.
  Super VL's [free route terms](https://openrouter.ai/nvidia/nemotron-3.5-super-vl:free)
  restrict confidential and personal data in arbitrary screenshots. Nano
  Omni lacks the required schema capability for structured vision and
  produced nine malformed coordinate responses. Both remain provisional.
  NVIDIA's direct signed-feed offers retain their existing availability rules.

## General benchmark coverage

The general grid contains 830 cells across 36 provider-qualified endpoints.
Numbers below are returned responses out of attempted cases. Coordinate
response counts include results that missed the reviewed target or failed its
confidence threshold. The summary retains the separate strict pass rates.

The registered run is `import-20261122T2000342743700000-a1c8eae52dfe`.
It contains 1,003 cells including supplemental localization. Codex directly
reviewed all 375 successful responses with rubrics, including 286 text and
89 localization responses. OCR returned 133/240 responses and coordinates
returned 147/240. The dated evidence directory contains the merged attempts,
structured agent reviews, and a recovery list for failed cells.

Gemma 4 31B's third localization case stalled for over ten minutes. Gemini 4
Hydrogen's second and third localization cases each hit the 120 s deadline.
Those supplemental shards were stopped after retaining their full general
grids. Localization cases 3–6 on Gemma and 4–6 on Hydrogen lack completed
records. OpenRouter daily quota also limited supplemental results and further
recovery. Signed-in AI Studio quota verification was blocked by browser URL
verification.

| Provider | Exact endpoint | Text | OCR | Coordinate |
| --- | --- | ---: | ---: | ---: |
| gemini-live | `gemini-2.5-flash-native-audio-preview-12-2025` | 10/10 | 9/10 | 4/10 |
| gemini-live | `gemini-3.1-flash-live-preview` | 10/10 | 10/10 | 10/10 |
| gemini-live | `gemini-3.8-live` | 10/10 | 10/10 | 10/10 |
| gemini-live | `gemini-3.8-live-extended-thinking` | 10/10 | 10/10 | 10/10 |
| google | `gemini-2.5-flash-lite` | 2/10 | 0/10 | 1/10 |
| google | `gemini-3-flash-preview` | 10/10 | 10/10 | 10/10 |
| google | `gemini-3.1-flash-lite` | 10/10 | 8/10 | 10/10 |
| google | `gemini-3.5-flash` | 10/10 | 1/10 | 10/10 |
| google | `gemini-3.5-flash-lite` | 9/10 | 10/10 | 10/10 |
| google | `gemini-3.6-flash` | 10/10 | 1/10 | 9/10 |
| google | `gemini-3.7-flash` | 1/10 | 1/10 | 1/10 |
| google | `gemini-3.8-flash` | 1/10 | 0/10 | 1/10 |
| google | `gemini-4-hydrogen` | 7/10 | 2/10 | 4/10 |
| google | `gemini-robotics-er-2-preview` | 10/10 | 10/10 | 10/10 |
| google | `gemma-4-26b-a4b-it` | 9/10 | 3/10 | 9/10 |
| google | `gemma-4-31b-it` | 8/10 | 1/10 | 5/10 |
| groq | `openai/gpt-oss-120b` | 10/10 | — | — |
| groq | `openai/gpt-oss-20b` | 10/10 | — | — |
| groq | `qwen/qwen3.8-27b` | 10/10 | 10/10 | 10/10 |
| nvidia | `nvidia/nemotron-3-ultra-550b-a55b` | 10/10 | — | — |
| nvidia | `nvidia/nemotron-3.5-bolt-e4b` | 10/10 | — | — |
| nvidia | `nvidia/nemotron-3.5-lightning-30b-a3b` | 10/10 | — | — |
| nvidia | `nvidia/nemotron-3.5-super-vl-120b-a12b` | — | 10/10 | 6/10 |
| nvidia | `poolside/laguna-xs-2.1` | 10/10 | — | — |
| openrouter | `baidu/ernie-5.5-flash-preview:free` | 8/10 | — | — |
| openrouter | `google/gemma-4-26b-a4b-it:free` | 0/10 | 0/10 | 0/10 |
| openrouter | `google/gemma-4-31b-it:free` | 0/10 | 0/10 | 0/10 |
| openrouter | `inclusionai/ling-3.2-flash:free` | 2/10 | 2/10 | 1/10 |
| openrouter | `liquid/lfm-3-1.7b:free` | 9/10 | — | — |
| openrouter | `nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free` | 10/10 | 10/10 | 1/10 |
| openrouter | `nvidia/nemotron-3-ultra-550b-a55b:free` | 10/10 | — | — |
| openrouter | `nvidia/nemotron-3.5-lightning:free` | 10/10 | — | — |
| openrouter | `nvidia/nemotron-3.5-super-vl:free` | 10/10 | 10/10 | 10/10 |
| openrouter | `particle-ai/particle-1:free` | 10/10 | 5/10 | 5/10 |
| openrouter | `recursive/flint-v1.0:free` | 10/10 | — | — |
| taalas | `taalas/llama-3.1-8b` | 10/10 | — | — |

## Validation

- All 25 updated catalog timing rows match decision-ready rows in the
  registered latest-run report.
- Windows managed `cargo check`, `cargo test`, Clippy with warnings denied,
  formatting, and `git diff --check` passed. The test suite reported 3,920
  passing unit tests, five passing integration tests and 59 ignored unit tests.
- Android catalog validation and the focused preset suite passed for both
  Full and Play. The complete suites reproduced the three
  `HistoryMediaRemovalRaceTest` failures recorded on November 18.
