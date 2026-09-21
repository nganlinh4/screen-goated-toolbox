# Catalog decisions — 2026-09-21, protocol 16

This checkpoint contains 1,070 ordinary attempts across 107 complete model/suite rows: 450 text, 310 coordinate and 310 OCR cells. It also contains 240 non-ranking structured-translation diagnostics. Providers returned 559 ordinary successes; all 276 successful text responses and all 73 successful structured responses received direct Codex reviews with ratings and authored-rubric checks. These reviews are not human sign-off.

The registered logical run is `import-20260921T0253104967005000000-9041e6302ec9`. Raw attempts, exact outputs, errors, reviews and reports are in `target/catalog-benchmark/2026-09-21-complete/`. Discovery evidence is in `target/catalog-benchmark/2026-09-21-discovery.json`.

## Applied decisions

- Keep Groq Qwen 3.8 as both generic heads. Text returned 10/10, reviewed quality 4.9/5 and a 273 ms full-result median. OCR returned 10/10, 0.971 normalized similarity and a 1,160 ms representative median. Its 0/10 strict coordinate result remains separate control evidence.
- Refresh 32 catalog text and representative OCR latency values from decision-ready protocol-16 rows. Vision uses only successful OCR inputs whose effective longest edge is at most 1024 px and requires at least four such samples.
- Keep Gemini 3.5 Flash Lite as the immediate image fallback. Move Gemini 3 Flash ahead of 3.1 Flash Lite: it returned 9/10 OCR results at 0.921 similarity and 2,166 ms representative latency, versus 8/10, 0.904 and 2,800 ms.
- Move Gemini 3.8 Flash ahead of Gemini 3.8 Live and 3.5 Flash in the late text stack. It returned 9/10, reviewed 4.9/5, at 3,343 ms; Live returned 10/10 at 4.7/5 and 4,228 ms; 3.5 Flash returned 10/10 at 4.9/5 and 11,042 ms.
- Withdraw Groq Compound and Compound Mini at their provider shutdown. The Search preset now uses Gemini 3.1 Flash Lite through an explicit per-block Google Search path; ordinary uses of that endpoint still omit provider tools.
- Add a provider-qualified withdrawal for NVIDIA `nvidia/nemotron-3-nano-omni-30b-a3b-reasoning`. It returned 0/10 usable OCR results and only one coordinate response, which failed strict verification.
- Admit no provisional endpoint. OpenRouter exhausted the shared free-model daily allowance during round 7, so later hard cases cannot establish complete reliability. Route-specific access, 400, 429 and upstream failures remain distinct findings.
- Keep Gemini 2.5 Flash Lite disabled and do not add Gemini 2.5 Flash or Pro. The authenticated inventories and public model page still list them, but this project receives access-lifecycle 404 responses directing new users to newer models.
- Keep Gemini 3.8 Live Extended Thinking out of the catalog. It completed 9/10 text cases at 4.3/5 and 13,704 ms, but still added unwanted preamble on the label task and returned an error apology instead of the six-line policy answer. Its OCR row was 9/10 at 0.778 similarity.

Shared Google project quota exhaustion and OpenRouter account quota exhaustion are access findings. They do not turn a successful output into a model-quality failure and do not retire established selectable rows by themselves.

## Core ranking evidence

| Endpoint row | Suite | Success | Reviewed quality / automatic score | Catalog latency |
| --- | --- | ---: | ---: | ---: |
| Groq Qwen 3.8 | Text | 10/10 | 4.9/5 | 273 ms |
| Groq Qwen 3.8 | OCR | 10/10 | 0.971 | 1,160 ms |
| Groq GPT-OSS 20B | Text | 10/10 | 4.7/5 | 445 ms |
| Groq GPT-OSS 120B | Text | 10/10 | 4.6/5 | 497 ms |
| Gemini 3.5 Flash Lite | OCR | 9/10 | 0.909 | 2,034 ms |
| Gemini 3 Flash | OCR | 9/10 | 0.921 | 2,166 ms |
| Gemini 3.1 Flash Lite | OCR | 8/10 | 0.904 | 2,800 ms |
| Gemini 3.8 Flash | Text | 9/10 | 4.9/5 | 3,343 ms |
| Gemini 3.8 Live | Text | 10/10 | 4.7/5 | 4,228 ms |
| Gemini 3.5 Flash | Text | 10/10 | 4.9/5 | 11,042 ms |
| OpenRouter dots 3 Note | Text | 6/10 | 4.8/5 | 1,185 ms |
| OpenRouter dots 3 Note | OCR | 5/10 | 0.859 | 1,486 ms |
| Taalas Llama 3.1 8B | Text | 10/10 | 3.4/5 | 234 ms |

Taalas remains extremely fast but does not enter the recommended chain: it selected the wrong label, omitted the rewrite constraint, left source-language text untranslated and failed the policy synthesis. Lightning-fast completion alone does not clear the quality floor.

## Candidate and live-feed ledger

| Provider / exact endpoint | Current evidence | Decision |
| --- | --- | --- |
| Google `gemini-2.5-flash-lite` | One text and one OCR success; predominant access-lifecycle 404s. | Remain disabled for this project. |
| Google `gemini-2.5-flash` | One text success; no usable vision result; predominant access-lifecycle 404s. | Do not add. |
| Google `gemini-2.5-pro` | 0/30 ordinary successes; access-lifecycle 404s. | Do not add. |
| Gemini Live `gemini-3.8-live-extended-thinking` | Text 9/10, 4.3/5, 13,704 ms; OCR 9/10, 0.778; coordinate 10/10, 0.6 strict. | Reject ordinary catalog admission for output quality and latency. |
| OpenRouter `qwen/qwen3.8-27b:free` | Text 0/10, OCR 0/10; one coordinate response failed strict verification; upstream 429s dominate. | Defer; no usable general evidence. |
| OpenRouter Inkling routes | 0 usable results; `INVALID_API_KEY`. | Blocked by route access. |
| OpenRouter `nex-agi/nex-n2.5-mini:free` | Text 6/10; vision 0/20 with persistent request-contract failures. | Reject vision; defer text after shared quota exhaustion. |
| OpenRouter `nex-agi/nex-n2.5-pro:free` | Text 6/10; coordinate 5/10; OCR 3/10. | Defer; incomplete hard-case evidence. |
| OpenRouter Ling text routes | Five or six text successes before the account ceiling; returned outputs were generally good. | Defer; shared quota prevents complete reliability evidence. |
| OpenRouter `inclusionai/ling-3.0-flash-vl:free` | Text 5/10, coordinate 3/10, OCR 4/10 at 0.861 similarity. | Defer; insufficient reliability and hard-case coverage. |
| OpenRouter `nvidia/nemotron-3.5-lightning:free` | 5/10 text; leaked a long internal-looking preamble on the label task; 34,965 ms median. | Reject current ordinary-output behavior. |
| Other OpenRouter provisional text routes | At most five successes before the shared ceiling, or persistent provider/upstream errors. | Defer without semantic rejection unless stated above. |
| NVIDIA `nvidia/nemotron-3-super-120b-a12b` | Text 9/10, 4.3/5, 817 ms. | Keep governed by the signed live feed. |
| NVIDIA `openai/gpt-oss-20b` | Text 10/10, 4.7/5, 10,968 ms. | Keep governed by the signed live feed. |
| NVIDIA `meta/muse-glimmer-30b` | OCR 7/10 at 0.999 similarity; coordinate 10/10 but 0.3 strict. | Keep governed by the signed live feed; no catalog veto. |
| NVIDIA `meta/llama-3.2-11b-vision-instruct` | OCR 7/10 at 0.815; no valid coordinate result. | Keep governed by the signed live feed; no general promotion. |
| NVIDIA `nvidia/nemotron-3-nano-omni-30b-a3b-reasoning` | OCR 0/10; coordinate 1/10 with 0 strict. | Withdraw from general routing. |

## Lifecycle and quota findings

Google's [model catalog](https://ai.google.dev/gemini-api/docs/models) still lists the Gemini 2.5 stable endpoints. The authenticated request result and Google's developer-forum staff explanation show that 2.5 access is restricted for projects without prior use, so this record treats the 404 as project access rather than claiming global retirement.

Groq's [deprecation record](https://console.groq.com/docs/deprecations) sets 2026-09-21 as the shutdown date for `groq/compound` and `groq/compound-mini`. Those rows and their default-search marker were removed together. Groq's current [vision documentation](https://console.groq.com/docs/vision) continues to name direct Qwen 3.8 as the supported multimodal endpoint, consistent with the clean direct-Groq benchmark.

OpenRouter's current free plan documents a shared daily request ceiling and 20 requests per minute. The runner stayed below the minute ceiling; the recorded `free-models-per-day` errors therefore identify account allowance exhaustion, not self-inflicted burst throttling.

## Validation contract

The catalog source, Windows consumers, Android generated consumers and shared parity fixtures must validate together. The final checkpoint runs catalog validation, focused Windows and Android tests, both Android flavor unit suites, Rust formatting/tests/clippy, and diff hygiene. No release, production promotion or runtime-bundle upload is part of this benchmark day.
