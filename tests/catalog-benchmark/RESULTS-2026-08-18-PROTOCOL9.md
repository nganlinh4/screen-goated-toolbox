# Protocol 9 catalog decision — 2026-08-18

Two complete protocol-9 text/OCR runs plus one coordinate run, 770 attempts total.
Cerebras was removed from the product before this benchmark day, so its rows are
absent. The registered history selects text and OCR from the second text/OCR run
and coordinate from the merged first run, per the newest-complete-run-per-model
and suite rule.

## Provider-wide latency shift

Google endpoints are materially slower than the 2026-08-10 baseline, and the shift
reproduced across two independent full runs roughly 30 minutes apart. Groq and
Taalas were stable in both, which rules out local network or machine degradation.

| Provider | run 1 median delta | run 2 median delta |
| --- | ---: | ---: |
| google-gtx | +251% | +169% |
| google | +43% | +55% |
| openrouter | +50% | +37% |
| gemini-live | +9% | +7% |
| groq | +5% | +2% |
| taalas | +3% | -1% |

Because the effect reproduced under identical conditions with stable controls, the
measured values are adopted rather than treated as transient congestion. Recheck on
the next benchmark day: if Google returns toward its 2026-08-10 values, this run's
absolute Google latencies should be superseded rather than averaged.

## Text

| Endpoint | Success | Automatic score | Median | P95 | Manual review |
| --- | ---: | ---: | ---: | ---: | --- |
| Groq GPT-OSS 20B | 10/10 | 0.793 | 0.503s | 0.674s | Fast; weakest required-term coverage of the leaders (0.782) |
| Google GTX | 10/10 | 0.824 | 0.554s | 1.049s | Translation only |
| Groq GPT-OSS 120B | 10/10 | 0.834 | 0.557s | 0.630s | All four difficulty-10 rubric criteria met; tightest CV in the run (0.135) |
| Taalas Llama 8B | 10/10 | 0.539 | 0.637s | 0.740s | Previously rejected for wrong-language and semantic failures; unchanged |
| OpenRouter Nemotron Omni | 5/10 | 0.788 | 0.883s | 1.171s | Reliability failure |
| Gemini 3.5 Flash Lite | 10/10 | 0.837 | 0.956s | 1.368s | Highest automatic score of the leaders; term coverage 0.882 |
| Gemini 3.1 Flash Lite | 10/10 | 0.836 | 1.128s | 1.917s | Term coverage 0.890 |
| Groq Compound Mini | 10/10 | 0.820 | 1.140s | 1.696s | Search-specific |
| Gemini 3 Flash | 10/10 | 0.805 | 1.370s | 2.017s | |
| Gemma 4 26B A4B | 10/10 | 0.857 | 1.495s | 2.186s | |
| Gemini Robotics ER1.6 | 10/10 | 0.851 | 1.535s | 1.719s | |
| Gemini 3.5 Flash | 10/10 | 0.874 | 1.574s | 10.354s | Long tail |
| Gemma 4 31B | 10/10 | 0.881 | 1.992s | 3.173s | Best required-term coverage (0.910) |
| Groq Compound | 7/10 | 0.641 | 2.067s | 2.883s | Search-specific; reliability failure |
| Gemini 3.1 Live | 10/10 | 0.828 | 2.772s | 3.293s | |
| Gemini 2.5 Live | 10/10 | 0.815 | 3.479s | 4.730s | |
| Gemini 3.6 Flash | 10/10 | 0.858 | 3.847s | 24.676s | Severely degraded; see below |

Groq GPT-OSS 120B becomes `default_text_model_id` and leads `text_to_text`. It is
41% faster than the previous default at the median, less than half its P95, the
most consistent endpoint measured, equal on required-term coverage, and carries a
1000/day quota against 500/day. Its manual difficulty-10 output satisfied every
rubric criterion. The chain then alternates providers so a single-provider incident
cannot take the top two slots:

`groq-gpt-oss-120b` → `gemini-3.5-flash-lite` → `groq-gpt-oss-20b` →
`gemini-3.1-flash-lite` → `openrouter-nemotron-omni` → `gemma-4-31b` → `gemma-4-26b`

OpenRouter Nemotron Omni is retained in the final third of the chain despite 5/10
reliability. Availability is a hard gate for leading a chain, not for occupying a
tail position, and this run demonstrated correlated Google-wide slowdown; a
partially available third provider at the tail is worth more than an
all-Google-and-Groq chain. Revisit if its reliability does not recover.

## General image and OCR

| Endpoint | Success | Mean OCR | Strict OCR | Small-image median | All-case P95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Groq Qwen 3.6 | 10/10 | 0.940 | 90% | 0.739s | 1.201s |
| Gemini 3.5 Flash Lite | 10/10 | 0.871 | 60% | 1.234s | 1.373s |
| Gemini 3.1 Flash Lite | 10/10 | 0.888 | 80% | 1.432s | 2.345s |
| Gemini 3 Flash | 10/10 | 0.939 | 90% | 1.748s | 3.245s |
| Gemma 4 31B | 10/10 | 0.898 | 80% | 2.430s | 2.556s |
| Gemini Robotics ER2 | 10/10 | 0.879 | 70% | 2.694s | 3.168s |
| Gemini Robotics ER1.6 | 10/10 | 0.963 | 90% | 2.863s | 3.700s |
| Gemma 4 26B A4B | 10/10 | 0.883 | 70% | 3.021s | 3.908s |
| Gemini 3.1 Live | 10/10 | 0.845 | 50% | 4.059s | 5.873s |
| Gemini 3.5 Flash | 10/10 | 0.866 | 70% | 4.512s | 7.950s |
| Gemini 2.5 Live | 10/10 | 0.509 | 10% | 4.615s | 7.649s |
| Gemini 3.6 Flash | 9/10 | 0.880 | 78% | 59.878s | 77.502s |
| OpenRouter dots-3 Note | 10/10 | 0.894 | 70% | 2.666s | 5.688s |
| OpenRouter Nemotron Omni | 9/10 | 0.870 | 33% | 0.887s | 1.309s |

The image chain keeps Gemini 3.5 Flash Lite first. OpenRouter dots-3 Note replaces
Nemotron Omni in the OpenRouter slot: it more than doubles strict-pass (70% against
33%) and completed every attempt. Groq Qwen 3.6 now posts the best accuracy and the
lowest latency of any vision endpoint, and its earlier 7.5-second tail has collapsed
to 1.2s; promoting it to lead the general image chain is a reviewed candidate for
the next decision, deliberately deferred here because it also changes
`default_image_model_id`.

## Coordinate grounding

| Endpoint | Request success | Accepted accuracy | Representative total |
| --- | ---: | ---: | ---: |
| Gemini Robotics ER2 | 10/10 | 100% | 6.290s |
| Gemini Robotics ER1.6 | 10/10 | 100% | 6.522s |
| Gemini 3 Flash | 10/10 | 100% | 5.162s |
| Gemini 3.5 Flash | 10/10 | 100% | 14.516s |
| Gemma 4 31B | 10/10 | 90% | 7.643s |
| Gemini 3.5 Flash Lite | 10/10 | 90% | 3.330s |
| Gemini 3.1 Flash Lite | 10/10 | 80% | 6.232s |
| Gemma 4 26B A4B | 8/10 | 10% | 4.745s |
| Gemini 3.1 Live | 10/10 | 10% | 8.690s |
| Gemini 2.5 Live | 2/10 | 10% | 7.218s |

`computer_control_grounding` is unchanged: Robotics ER2 primary at 10/10 accepted,
Gemini 3.5 Flash Lite fallback at 90% and by far the fastest. This reproduces the
2026-08-10 result. The Live endpoints again failed the fail-closed grounding
contract, confirming they must stay out of authority-bearing paths.

## Gemini 3.6 Flash

Five independent signals in one day: text median 3.847s with a 24.676s P95 (8.503s
median in the first run); OCR 59.878s with a 77.502s P95 at 9/10; a coordinate
transport timeout; grounding calls of 26–65s even after its request timeout was
raised; and too few representative OCR cases in the first run to derive a catalog
latency at all. It is removed from `image_to_text`. Its rows remain enabled and
user-selectable because the endpoint still returns correct answers; only its
latency has collapsed.

## Candidate evaluation

The OpenRouter free inventory was enumerated in full: 19 zero-price endpoints out of
413 total. Three entered the catalog provisionally to exercise the production
dispatcher.

- `dots-studio/dots-3-note-preview:free` — accepted as `openrouter-dots-3-note-vision`.
  Strict schema support was verified with a live production-path probe before
  cataloging rather than inferred from its declared `supported_parameters`.
- `nvidia/nemotron-3-nano-30b-a3b:free` — rejected. Required-term coverage 0.627, and
  its difficulty-10 output contained corrupted tokens (`không thểapse`).
- `nvidia/nemotron-3-super-120b-a12b:free` — rejected. Term coverage 0.805, P95
  5.660s, latency CV 1.342, and it added unrequested glosses while altering the
  Markdown placeholder the case required preserving.

Both rejected candidates screened far faster than they benchmarked (0.319s and
0.533s on a single easy sentence). Single-prompt screening is candidate-selection
evidence only and must never be used to rank endpoints.

Rejected without cataloging: `openai/gpt-oss-20b:free` returns HTTP 400 "Reasoning is
mandatory for this endpoint and cannot be disabled", so the same upstream model Groq
serves cannot honour the catalog reasoning policy through OpenRouter;
`z-ai/glm-5.2:free` returned 429 on three of four probes;
`nvidia/nemotron-nano-9b-v2:free` took 20.8s and emitted reasoning despite
`effort: none`; `poolside/laguna-s-2.1:free` ignored `response_format`;
`nvidia/nemotron-nano-12b-v2-vl:free` took 23s and dropped diacritics.

SambaNova was evaluated as a provider and rejected: all six endpoints return HTTP 402
`PAYMENT_METHOD_REQUIRED` with a zero balance, failing the no-billing requirement.

## Discovery, lifecycle, and quota notes

Authenticated inventories confirmed every cataloged Gemini and Groq endpoint is live,
with no deprecation metadata on any of them. All 20 quota labels were verified
against the signed-in AI Studio rate-limit page and Groq's official free-plan table
plus live response headers; none required a change.

`gemini-3.7-flash` is available on the free tier at the same 5 RPM / 250K TPM /
20 RPD allowance as 3.5 and 3.6 Flash, and answers text and vision correctly. It is
not cataloged: it rejects `thinkingLevel: MINIMAL` with HTTP 400, so it needs a
reasoning policy the manifest does not currently express.
`gemini-omni-flash-preview` shows 0 RPM / 0 TPM / 0 RPD on the free tier and returned
429 on two independent unused keys.

Search grounding is 0 RPD for the entire Gemini 3 family on this tier, which
independently supports keeping `search_tool_enabled_by_default` false for those
endpoints.

## Run integrity disclosures

- The five recovered Gemini 3.6 Flash coordinate cells were measured with
  `CATALOG_BENCH_REQUEST_TIMEOUT_SECS=180` after the first coordinate run aborted,
  while every other cell used the 120s default. The deviation favours that endpoint
  and does not affect its removal from the chain.
- The first coordinate run terminated abnormally at round 7 without writing
  `run.json`. Its 70 completed cells were preserved and combined with two focused
  recoveries into a single registered logical run; no fragment was registered
  separately.
- The second text/OCR run exhausted the OpenRouter free daily allowance
  (`Rate limit exceeded: free-models-per-day`) because three passes ran in one day.
  Both OpenRouter vision rows fell below the four-case representative minimum in that
  run, so their catalog latency is carried from the merged first run where both
  completed cleanly. This is recorded in `performance_source` as
  `…:ocr-small-1024-merged`. The dots-3 acceptance holds in both runs independently
  (70% against 33%, and 62.5% against 28.6% strict-pass).
