# Catalog benchmark — 2026-08-20 (protocol 11), OCR reliability run

The clean run the previous record could not provide. 150 attempts over 15 rows,
with the per-provider interval raised to 3s. This owns the image chain order and
every vision latency; text, coordinate and localization rows are unchanged.

## One case fails for the whole Gemini family, twice running

Nine endpoints failed on the same case in the same round, `ocr-04-vietnamese-web-layout`,
in both runs and despite a longer per-provider interval, each answering
`429 RESOURCE_EXHAUSTED`.

**The cause is not known.** Three explanations were tested and all three failed:

- *Payload size.* This case produces the smallest crop in the suite at 90KB.
  `ocr-09` at 1316KB and `ocr-01` at 1079KB are an order larger and never fail.
- *Key concentration.* The benchmark pool is round-robin per call, so a round uses
  as many distinct keys as it has models, and the rotated key does reach the
  request. Nine large images across nine keys, and nine on a single key, all
  return 200.
- *The case itself.* Reproducing the exact benchmark payload — same crop, same PNG
  encoding, same instruction, nine models on nine keys concurrently — returns nine
  successes.

It reproduces only inside a full run, which suggests something cumulative across
the run rather than anything about this case. That remains open.

What can be said is narrower: the outage is identical for every endpoint, so it
cannot separate them, and the ordering below excludes that case while keeping
failures that differ between endpoints.

## Image chain

| # | Endpoint | Reliability | Median | Mean | Strict |
| ---: | --- | ---: | ---: | ---: | ---: |
| 0 | `groq-qwen-3-6-27b-vision` | 100% | 0.85s | 0.920 | 70% |
| 1 | `google-gemini-3-5-flash-lite-vision` | 100% | 1.41s | 0.871 | 67% |
| 2 | `google-gemini-3-5-flash-vision` | 100% | 1.61s | 0.866 | 78% |
| 3 | `google-gemini-3-flash-vision` | 100% | 2.10s | 0.883 | 78% |
| 4 | `google-gemini-3-1-flash-lite-vision` | 100% | 2.17s | 0.874 | 78% |
| 5 | `openrouter-dots-3-note-vision` | 100% | 2.56s | 0.869 | 50% |
| 6 | `google-gemini-robotics-er-1-6-vision` | 100% | 3.66s | 0.880 | 78% |
| 7 | `google-gemini-robotics-er-2-vision` | 100% | 3.83s | 0.868 | 78% |
| 8 | `google-gemma-4-26b-a4b-vision` | 100% | 4.07s | 0.870 | 78% |
| 9 | `google-gemma-4-31b-vision` | 100% | 4.16s | 0.893 | 80% |
| 10 | `google-gemini-3-6-flash-vision` | 100% | 7.44s | 0.826 | 67% |
| 11 | `google-gemini-3-7-flash-vision` | 89% | 13.68s | 0.877 | 75% |

Eleven of the twelve are fully reliable once the shared outage is removed, so the
order is latency-weighted within that group. Eight endpoints move. The largest
corrections are `gemini-3.1-flash-lite` and `dots-3 Note`, which were sitting at 10
and 9 on stale figures and measure 2.17s and 2.56s, and `gemini-3.7-flash`, which
drops from 5 to the tail on 89% reliability and a 13.68s representative median.

Qwen 3.6 holds the lead and improves to 846ms at 0.920 mean. The default image model
follows the chain head, unchanged.

## What was not touched

Text, coordinate and localization rows still rest on the
[2026-08-19 protocol-10 run](RESULTS-2026-08-19-PROTOCOL10.md); only the OCR request
shape had changed. The text chain order is deliberate rather than latency-sorted:
`groq-gpt-oss-20b-text` sits at 8 despite 527ms because it carries the lowest
translation quality of any enabled text row.
