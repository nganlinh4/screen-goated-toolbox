# Vision latency-tail decision — 2026-07-26

This reviewed diagnostic reran all ten protocol-6 OCR levels through the exact
production Rust vision entry point. It interleaved seven priority-relevant
endpoints round-by-round and retained every failed attempt. The output remains
local at
`target/catalog-benchmark/diagnostics/20260726-production-ocr-r1`; it was not
registered as a replacement for the complete protocol-6 catalog history.

| Endpoint | Success | Small median | Small P95 | All-case P95 | Successful-call CV |
| --- | ---: | ---: | ---: | ---: | ---: |
| OpenRouter Nemotron Omni | 10/10 | 0.845 s | 1.103 s | 4.534 s | 0.830 |
| Gemini 3.1 Flash Lite | 10/10 | 0.895 s | 1.001 s | 1.500 s | 0.238 |
| Gemini 3.5 Flash Lite | 10/10 | 0.937 s | 1.033 s | 1.546 s | 0.253 |
| Groq Qwen 3.6 27B | 9/10 | 1.327 s | 2.067 s | 4.483 s | 0.666 |
| Cerebras Gemma 4 31B | 10/10 | 1.511 s | 2.534 s | 4.741 s | 0.586 |
| Google Gemma 4 31B | 10/10 | 2.462 s | 9.473 s | 8.453 s | 0.667 |
| Gemini 3.6 Flash | 3/10 | 1.355 s | 1.369 s | 4.643 s | 0.669 |

Gemini 3.6's seven failures were the documented 20-request daily quota and do
not provide enough successful evidence for promotion.

The priority-changing observation was Groq Qwen's failed second attempt. Groq
held the request for 31.4 seconds before returning HTTP 503 with an explicit
over-capacity message. Successful-latency percentiles exclude failed attempts,
so neither the 2.067-second small P95 nor the 4.483-second all-case P95 exposes
that fallback-blocking delay. A controlled rerun of the same 1280×857 source
later completed in 1.915 seconds: 0.462 seconds of local preparation and 1.408
seconds in the provider call. The variation is provider capacity, not a
different benchmark/product request path.

The general image retry chain therefore keeps Gemini 3.5 Flash Lite first and
promotes the stable Gemini 3.1 Flash Lite second, followed by OpenRouter
Nemotron Omni and Cerebras Gemma. Groq Qwen moves behind those four endpoints.
The remaining lower-confidence, quota-constrained, accuracy-specialized, and
slow-tail endpoints preserve their relative order. This follows the catalog
policy that availability and consistency are hard gates before successful-call
latency and small accuracy differences.

This decision changes only the canonical general image retry chain. It does not
replace a model explicitly selected by a preset or user.
