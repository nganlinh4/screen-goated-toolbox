# Model discovery — 2026-11-23

The API-first inventory and quota evidence are under
`target/catalog-benchmark/day-2026-11-23/`. The Windows production benchmark
used protocol 17 from this checkout. The hosted workflow's default branch
still compiled protocol 16.

## Google

All 20 configured credentials listed the same 65 exact IDs. The pricing page
loaded, but the parser produced no model-level price matches for its current
tables. Google's published pricing, model and deprecation pages supplied the
route classifications. Chrome URL verification blocked access to the signed-in
AI Studio limits page, so the run did not update project quota labels.
[Pricing](https://aistudio.google.com/docs/pricing) ·
[Project limits](https://aistudio.google.com/docs/rate-limits)

| Endpoint change or candidate | Disposition |
| --- | --- |
| `gemini-4-neon` | New in the authenticated inventory. Paid standard input and output. [Pricing](https://aistudio.google.com/docs/pricing#gemini-4-neon) |
| `genie-4` | New in the authenticated inventory. Interactive world generation over the Interactions API, with live video and audio output. [Genie 4](https://aistudio.google.com/docs/models/genie-4) |
| `deep-research-pro-preview-12-2025` | Absent from today's list. Its managed research contract was already outside the preset benchmark. |
| `gemini-4-argon`, `gemini-4-carbon` | Retain paid-only disposition. [Pricing](https://aistudio.google.com/docs/pricing) |
| `gemini-4-hydrogen` | Retested with its minimum `LOW` reasoning policy and Gemini GenerateContent text/vision profiles. [Model contract](https://aistudio.google.com/docs/models/gemini-4-hydrogen) |
| `gemini-2.5-flash-lite` | Retested disabled text and vision rows. The model list still exposed the ID, while live calls returned project-access HTTP 404s. [Deprecations](https://aistudio.google.com/docs/deprecations#gemini-2.5-flash-models) |
| `gemini-2.5-flash`, `gemini-2.5-pro` | Restricted legacy access, with provider rerouting scheduled for December 1. The active pool lacks current evidence of otherwise eligible legacy access. [Deprecations](https://aistudio.google.com/docs/deprecations) |
| `gemini-3.1-flash-lite-preview` | Still listed. Google documents shutdown on May 25 and a stable replacement. [Model notice](https://aistudio.google.com/docs/models/gemini-3.1-flash-lite-preview) |
| `gemma-4-26b-a4b-it` | Retested the enabled text and disabled vision profiles. |
| `gemini-3.8-live-extended-thinking` | Retested through the Live adapter using `LOW` and interaction-idle completion. [Live pricing](https://aistudio.google.com/docs/pricing#gemini-3.8-live) |

The [November 18](DISCOVERY-2026-11-18.md) and
[October 18](DISCOVERY-2026-10-18.md) ledgers cover the remaining pre-existing
aliases, generation, embedding, safety, managed-agent and dedicated audio or
session endpoints. Enabled ordinary models were retested through their
catalog profiles.

## Groq, NVIDIA and Taalas

- Both Groq credentials returned the same 11 IDs as November 18. The three
  documented general Free Plan models were retested: `openai/gpt-oss-20b`,
  `openai/gpt-oss-120b` and `qwen/qwen3.8-27b`. `allam-2-7b` remains
  API-visible without published Free Plan eligibility. The other endpoints
  serve speech or moderation. [Groq Free Plan limits](https://console.groq.com/docs/rate-limits)
- The verified NVIDIA feed generated `2026-11-22T12:39:02Z` offered
  `poolside/laguna-xs-2.1`, `nvidia/nemotron-3.5-bolt-e4b`,
  `nvidia/nemotron-3.5-lightning-30b-a3b`, `nvidia/nemotron-3-ultra-550b-a55b`
  and `nvidia/nemotron-3.5-super-vl-120b-a12b`. The runner exercised all five
  offers at their signed reasoning policies. `minimaxai/minimax-m3.1-flash`,
  offered on November 18, is absent from today's feed. NVIDIA GPT-OSS and
  Nemotron 3 Super remain withheld. [NVIDIA hosted LLM API](https://docs.api.nvidia.com/nim/reference/llm-apis)
- Taalas `taalas/llama-3.1-8b` ran its text grid through the existing preset
  adapter.

## OpenRouter

Fourteen IDs had zero prompt and completion prices. `openrouter/free` is a
floating router and both Lyria IDs serve dedicated music generation. The 11
fixed general endpoints were selected for one provider-wide shard at a time.
Three configured free accounts initially reported 50 requests each. [Model
inventory](https://openrouter.ai/api/v1/models) ·
[Request limits](https://openrouter.ai/docs/api_reference/limits)

`dots-studio/dots-3-note-preview:free`, `cohere/north-mini-code:free`, and
`thinkingmachines/inkling-2-small:free` disappeared from the zero-price list
since November 18. OpenRouter returned an empty serving-endpoint array for
Dots 3 Note's exact free route. Both SGT rows were disabled and removed from
default retry chains. [Exact route availability](https://openrouter.ai/api/v1/models/dots-studio/dots-3-note-preview:free/endpoints)

The OpenRouter route for `nvidia/nemotron-3.5-super-vl:free` documents a
restriction on confidential data and personal data, including faces. Its
successful public-fixture OCR results do not establish suitability for a
shipped retry chain that handles arbitrary screenshots.
[Route terms](https://openrouter.ai/nvidia/nemotron-3.5-super-vl:free)
