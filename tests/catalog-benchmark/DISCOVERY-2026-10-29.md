# Discovery ledger — 2026-10-29

This ledger records changes since the [2026-10-18 ledger](DISCOVERY-2026-10-18.md) and closes every eligible endpoint for this run. Any identity not listed here keeps its 2026-10-18 disposition, because its inventory and exclusion facts are unchanged: the Gemini inventory fingerprint is identical, and the Groq model list is identical. The [applied decision record](RESULTS-2026-10-29-PROTOCOL16.md) owns ordering and eligibility.

Observed 2026-10-29 at about 11:07 UTC through these sources:
- authenticated `GET /v1beta/models` (Gemini, 20/20 credentials);
- `GET /openai/v1/models` (Groq, 2/2 credentials, from hosted runners);
- `GET https://openrouter.ai/api/v1/models` (zero prompt and completion pricing);
- the signed NVIDIA availability feed, refreshed by the benchmark.

Credentials are excluded from evidence.

## Changes since 2026-10-18

| Provider | Exact endpoint | Change | Disposition |
| --- | --- | --- | --- |
| openrouter | `recursive/flint-v1.0:free` | New zero-priced text route | Benchmarked through an isolated production-path candidate profile. Not admitted: structured output failed 0/6 because of mandatory reasoning, and it has no advantage over the dots 3 Note tail. |
| openrouter | `nvidia/nemotron-3-super-120b-a12b:free` | Free route removed; only the paid slug remains | Withdrawn from the catalog. Live request: HTTP 404 "unavailable for free". |
| google | `veo-3.1-generate-preview`, `veo-3.1-fast-generate-preview`, `veo-3.1-lite-generate-preview` | No longer listed | Previously excluded paid video generation; no catalog impact. |

## Retested general endpoints (this run)

These endpoints received the complete production-path grid, with per-suite counts in the decision record:
- **gemini-live:** 2.5 native audio, 3.1 Flash Live and 3.8 Live.
- **google:** 3 Flash, 3.1 Flash Lite, 3.5 Flash Lite, 3.5 Flash, 3.6 Flash, 3.7 Flash, 3.8 Flash, Robotics-ER-2, Gemma 4 26B and Gemma 4 31B.
- **groq:** Qwen 3.8 27B, GPT-OSS 20B and GPT-OSS 120B.
- **openrouter:** dots 3 Note, Gemma 4 26B free and Flint (candidate). Nemotron 3 Super free was attempted and returned only 404s.
- **nvidia (signed feed):** MiniMax M3.1 Flash, Nemotron 3.5 Lightning, Nemotron 3.5 Bolt E4B, Nemotron 3.5 Super VL and Nemotron 3 Ultra.
- **taalas:** Llama 3.1 8B.

## Explicit blockers and deferrals

**NVIDIA rows not offered by the feed.** The catalog rows for NVIDIA GPT-OSS 20B/120B and Nemotron 3 Super were not offered by the signed feed during this run. Feed governance keeps them out of routing; their historical metadata is unchanged.

**OpenRouter routes rejected on 2026-10-18, not retested.** These are Xing 4.0, Nemotron 3.5 Super VL free, LFM 2.5, Nemotron 3.5 Lightning free, Inkling Small, Laguna S 2.1, Inkling, Laguna XS 2.1, North Mini Code, Nemotron 3 Ultra free, Nemotron 3 Nano Omni free and Gemma 4 31B free. OpenRouter's free allowance is account-wide: about 98 requests remained after the baseline and the candidate, which cannot cover their grids. Their 2026-10-18 dispositions stand. This is a quota deferral, not a new rejection.

**Groq from this PC.** Requests are blocked at the network edge (HTTP 403 "Access denied. Please check your network settings", with or without a credential). Hosted runners have full access; Groq evidence comes from hosted runs only.

**Legacy Gemini 2.5 Flash, Flash Lite and Pro; Live 3.8 Extended Thinking; session-contract endpoints; Groq `allam-2-7b`.** The access, lifecycle, contract and free-tier findings from 2026-10-18 are unchanged and were not reopened.
