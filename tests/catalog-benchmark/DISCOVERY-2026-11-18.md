# Model discovery — 2026-11-18

The API-first inventory was collected at 04:12 KST. The raw report and
credential-free quota snapshots are under
`target/catalog-benchmark/2026-11-18-discovery/` and
`target/catalog-benchmark/day-2026-11-18/`.

- Gemini returned 64 endpoints. All 20 configured credentials returned the same
  inventory (`sha256:8b822bd1…`), including every current catalog endpoint.
  The pricing parser did not recognize the redesigned page. The benchmark
  checked Google's published Markdown tables directly. The browser bridge was
  unavailable during the run. The user later supplied the signed-in
  Project 1 rate-limit table, recorded below.
  [Google pricing](https://aistudio.google.com/docs/pricing)
  [Google project rate limits](https://aistudio.google.com/docs/rate-limits)
- Groq returned 11 endpoints. Both configured credentials returned the same
  inventory (`sha256:75bff592…`). The free-plan general endpoints are
  `openai/gpt-oss-20b`, `openai/gpt-oss-120b`, and `qwen/qwen3.8-27b`.
  `allam-2-7b` remains API-visible without documented free eligibility.
  The other listed endpoints are speech and moderation services.
  [Groq models](https://console.groq.com/docs/models)
  [Groq Free Plan limits](https://console.groq.com/docs/rate-limits)
- OpenRouter returned 17 entries with zero prompt and completion prices. Three
  independent free counters each started at 50 remaining requests and advanced
  by different amounts. Requests were serialized across the provider pool.
  `openrouter/free` is a floating router, and Google's two Lyria entries are
  music generation. The Inkling 2 Small free route is restricted to agentic
  harnesses and falls outside the ordinary SGT preset contract.
  [OpenRouter inventory](https://openrouter.ai/api/v1/models)
  [OpenRouter limits](https://openrouter.ai/docs/api_reference/limits)
  [Inkling 2 Small route terms](https://openrouter.ai/thinkingmachines/inkling-2-small:free)
- The production runner verified NVIDIA's signed feed generated
  2026-11-17 13:32 UTC. It offered `poolside/laguna-xs-2.1`,
  `nvidia/nemotron-3.5-lightning-30b-a3b`, `nvidia/nemotron-3.5-bolt-e4b`,
  `nvidia/nemotron-3.5-super-vl-120b-a12b`,
  `nvidia/nemotron-3-ultra-550b-a55b`, and `minimaxai/minimax-m3.1-flash`.
  The catalog's NVIDIA GPT-OSS and Nemotron 3 Super rows were withheld by the feed.
  [NVIDIA hosted LLM API](https://docs.api.nvidia.com/nim/reference/llm-apis)

## Gemini Free tier limits for Project 1

These are the limit values shown in the user-supplied AI Studio table with
the one-day view selected. The table reports peak usage alongside each limit.
The limits apply to Project 1. Other benchmark projects remain unverified.

| Console model | RPM | Input TPM | RPD |
| --- | ---: | ---: | ---: |
| Gemini 3 Flash, 3.6 Flash, 3.8 Flash, Robotics ER 2 Preview, 4 Hydrogen | 5 | 250,000 | 20 |
| Gemini 2.5 Flash Lite | 10 | 250,000 | 20 |
| Gemini 3.1 Flash Lite, 3.5 Flash Lite | 15 | 250,000 | 500 |
| Gemma 4 26B, 4 31B | 30 | 16,000 | 14,400 |
| Gemini 2.5 Flash Native Audio Dialog | Unlimited | 1,000,000 | Unlimited |
| Gemini 3 Flash Live, 3.8 Live, 3.8 Live Extended Thinking | Unlimited | 65,000 | Unlimited |
| Antigravity | 60 | 100,000 | 100 |

The displayed daily limits agree with the corresponding catalog labels.
Gemini 4 Hydrogen's 20 RPD confirms it was available on Project 1's Free
tier. Its observed peak daily usage was 2 of 20. That single-project reading
leaves the cause of the run's individual 429s unresolved.

### HTTP 429 follow-up

An authenticated Google Cloud key lookup mapped the 20 configured Gemini
credentials to 20 distinct projects. The benchmark's recorded OCR recovery
offset and stable credential order place the four Hydrogen OCR failures
(rounds 2–5) on four projects other than Project 1. The original logs contain
HTTP 429 `RESOURCE_EXHAUSTED` without a quota ID, structured detail, or
retry hint for those four responses.

Cloud Monitoring also recorded 31 Gemini Developer API 429s across ten
projects, including Project 1, during 19:22–19:24 UTC on November 17
(04:22–04:24 KST on November 18). Both GenerateContent and
StreamGenerateContent appear in those counts, which do not identify the
model. The corresponding `quota/exceeded` query returned no time series.
The cross-project cluster is consistent with temporary service-side
throttling, but the evidence does not identify the specific trigger. The
sanitized historical query results are in
`target/catalog-benchmark/day-2026-11-18/gemini-429-cloud-monitoring.json`.
See [Google project rate limits](https://aistudio.google.com/docs/rate-limits)
and [Cloud API monitoring](https://docs.cloud.google.com/apis/docs/monitoring).

### November 20 recheck

Two streaming OCR diagnostics on the billing-disabled Project 1 returned
HTTP 503 `UNAVAILABLE` for Gemini 4 Hydrogen at 00:59 KST. Google attributed
both responses to high demand. The Gemini 3.1 Flash Lite control timed out
while waiting for the response, then returned the same high-demand 503 on
retry. This identifies a capacity problem affecting both models during the
recheck. The exact trigger for the November 18 HTTP 429s remains unresolved.

From midnight KST through the monitoring queries shortly after 01:00, Cloud
Monitoring returned no Gemini Developer API 429 or quota-exceeded series for
Project 1 and the four earlier recovery projects. Recent metrics can arrive
after the query. The four other projects were checked through historical
metrics only because the Cloud Billing API returned 429 when verifying their
billing status. These focused diagnostics are outside the catalog benchmark.
Sanitized evidence is under
`target/catalog-benchmark/hydrogen-recheck-2026-11-20/`.

## Gemini discoveries and contract exclusions

| Exact endpoint | Evidence |
| --- | --- |
| `gemini-4-hydrogen` | Free standard input and output, text output, image input, structured output, and a minimum `LOW` thinking level. Its provisional profile was exercised through the existing GenerateContent adapter. |
| `gemini-4-argon`, `gemini-4-carbon` | Listed by every credential. The official standard input and output pricing has no free tier. |
| `gemini-omni-1.2-flash` | Dedicated video generation contract. |
| `gemini-decision-1` | Dedicated Decisions API with categorical outputs and zero billed output tokens. |
| `gemini-2.5-flash-lite` | Disabled candidate retested. Some projects returned normal answers, while others returned the documented legacy-access 404. |
| `gemini-2.5-flash`, `gemini-2.5-pro` | Legacy project access restrictions apply. Google documents automatic rerouting on December 1. |

Sources for the above are [Gemini 4 Hydrogen](https://aistudio.google.com/docs/models/gemini-4-hydrogen),
[Gemini 4 reasoning levels](https://aistudio.google.com/docs/thinking#understanding-thinking-levels),
[pricing](https://aistudio.google.com/docs/pricing), and
[deprecations](https://aistudio.google.com/docs/deprecations).
The [October 18 ledger](DISCOVERY-2026-10-18.md) documents the exact identities
and prior dispositions of the pre-existing generation, audio, embedding,
safety, managed-agent, and incompatible session endpoints.

## OpenRouter endpoint ledger

| Exact endpoint | Today's disposition |
| --- | --- |
| `dots-studio/dots-3-note-preview:free` | Retested the full text, OCR and coordinate grids. |
| `google/gemma-4-26b-a4b-it:free` | Retested the full grid. All 30 base cells returned upstream 429s while the OpenRouter accounts still had available daily requests. |
| `inclusionai/ling-3.2-flash:free` | New endpoint. Full text, OCR and coordinate grids with the route's non-thinking request control. |
| `baidu/ernie-5.5-flash-preview:free` | New text endpoint. The initial effort-none request failed with HTTP 400. The corrected profile omits the control because reasoning is mandatory. All ten baseline text cells then returned, while structured diagnostics returned no usable result. [Baidu API contract](https://cloud.baidu.com/doc/qianfan-api/s/wmwbl48n4) |
| `liquid/lfm-3-1.7b:free` | New text endpoint. Full text grid, seven returned. No structured diagnostic returned usable output. |
| `particle-ai/particle-1:free` | New endpoint. Full text grid returned. A complete OCR and coordinate grid exceeded the remaining free quota. |
| `cohere/north-mini-code:free` | Retested text. Direct review found unrequested English reasoning in one translation and extra output in a structured localization case. |
| `recursive/flint-v1.0:free` | Retested text with its mandatory reasoning. All ten text cells returned, with a long tail and no usable structured diagnostic. |
| `google/gemma-4-31b-it:free`, `nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free`, `nvidia/nemotron-3-ultra-550b-a55b:free`, `nvidia/nemotron-3.5-lightning:free`, `nvidia/nemotron-3.5-super-vl:free` | Still advertised at zero price. Previous rejections or deferrals are recorded in the October 18 ledger. Complete current grids exceeded quota. |
| `google/lyria-3-clip-preview`, `google/lyria-3-pro-preview` | Dedicated music generation. |
| `thinkingmachines/inkling-2-small:free` | Route terms restrict the free endpoint to agentic harnesses. |
| `openrouter/free` | Floating router with no stable model identity. |

The main OpenRouter grid left 113 requests across the three account counters.
After the first candidate group, 42 remained. The supplemental text group
automatically ran six localization requests for each selected text endpoint,
including the two other text endpoints selected for the base grid.
Some final diagnostic cells returned account quota errors. Three requests
remained across the pool, below the minimum for another ten-case text grid.

Previously reviewed routes absent from the current zero-price inventory
include `liquid/lfm-2.5-2.6b:free`, `poolside/laguna-s-2.1:free`,
`poolside/laguna-xs-2.1:free`, `thinkingmachines/inkling-small:free`,
`thinkingmachines/inkling:free`, `xingchen-agi/xing4.0-29b-a4b:free`, and
`nvidia/nemotron-3-super-120b-a12b:free`. The OpenRouter Laguna XS route and
the current signed NVIDIA offer are distinct provider endpoints.
