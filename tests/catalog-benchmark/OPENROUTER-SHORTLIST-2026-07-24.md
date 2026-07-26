# OpenRouter free-model shortlist — 2026-07-24

The live OpenRouter Models API returned 18 text-output entries whose prompt and
completion prices were both zero. The API was queried twice with its
server-owned `latency-low-to-high` and `throughput-high-to-low` sorts. Those
sorts use recent p50 routing heuristics; they are discovery signals, not
substitutes for the product benchmark's full-answer completion time.

Content-safety models, audio-generation endpoints, and the random
`openrouter/free` router were excluded from general text/image candidacy.

| Model | Latency rank | Throughput rank | Current evidence | Decision |
| --- | ---: | ---: | --- | --- |
| Nemotron 3 Nano Omni 30B A3B | 2 | 1 | Product benchmark complete with reasoning off | Admitted provisionally |
| Nemotron 3 Nano 30B A3B | 3 | 3 | 97.30% one-day endpoint uptime; lower published intelligence than Super | Benchmark only if the Omni text route proves insufficient |
| Nemotron 3 Super 120B A12B | 9 | 4 | Product text benchmark: 9/10, 1.701s median, 120s timeout, serious fidelity failures | Rejected |
| Ling 3.0 Flash | 12 | 5 | Product text benchmark: 10/10, 84.939%, 0.901s median, tight latency range | Candidate for user review |
| Gemma 4 31B | 7 | 7 | Same upstream model already cataloged through Google and Cerebras | Skip duplicate provider route for now |
| Nemotron Nano 12B v2 VL | 10 | 13 | Product vision benchmark: 2/9 coordinates, 86.871% OCR, 3.610s small-image OCR median | Rejected |

Official sources:

- [OpenRouter Models API sorting contract](https://openrouter.ai/docs/guides/overview/models)
- [Nemotron 3 Super free performance](https://openrouter.ai/nvidia/nemotron-3-super-120b-a12b%3Afree/performance)
- [Ling 3.0 Flash free model page](https://openrouter.ai/inclusionai/ling-3.0-flash%3Afree)
- [Nemotron Nano 12B v2 VL free performance](https://openrouter.ai/nvidia/nemotron-nano-12b-v2-vl%3Afree/performance)

## Live benchmark result

The renewed key enabled the full reasoning-off product benchmark. See
[`RESULTS-2026-07-24-OPENROUTER-SHORTLIST.md`](RESULTS-2026-07-24-OPENROUTER-SHORTLIST.md).
Temporary candidate entries were removed after the run. Ling remains the only
candidate awaiting user approval; Super and Nano VL are rejected.
