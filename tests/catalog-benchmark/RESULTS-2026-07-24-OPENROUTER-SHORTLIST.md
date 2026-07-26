# OpenRouter free shortlist benchmark — 2026-07-24

This decision record evaluates the three candidates discovered through
OpenRouter's live free-model latency and throughput sorts. Candidate entries
were staged only for the benchmark and removed afterward.

All requests used the production Rust paths with nested
`reasoning: { "effort": "none" }`. The helper enforced a 3.1-second
OpenRouter cadence. The text models saw all ten translation difficulties in
round-major order. The vision model saw all ten coordinate and OCR
difficulties.

## Reasoning-off admission probes

| Model | Status | Completion | Result | Reasoning |
| --- | --- | ---: | --- | ---: |
| Nemotron 3 Super 120B A12B | 200 | 0.911s | Correct negation and backup meaning | 0 tokens |
| Ling 3.0 Flash | 200 | 0.773s | Correct negation and backup meaning | 0 tokens |

## Text benchmark

| Model | Reliability | Automatic accuracy | Median | P95 | TTFO median | Latency CV |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Ling 3.0 Flash | 10/10 | 84.939% | 0.901s | 1.224s | 0.825s | 0.167 |
| Nemotron 3 Super 120B A12B | 9/10 | 66.075% | 1.701s | 9.065s | 0.491s | 1.228 |
| Nemotron 3 Nano Omni 30B A3B | 10/10 | 75.899% | 0.437s | 0.821s | 0.175s | — |

Manual Ling review found seven strong translations, one partial result, and
two serious fidelity defects:

- it weakened `Caution` to `Note` while preserving the required two-line menu
  structure;
- it invented a feminine pronoun for an unknown-gender person and changed a
  failed backup into a corrupted backup;
- it changed the proper name `Marta` to `Martha`.

Ling otherwise preserved the numeric distinction, negative threshold,
placeholders, date, deletion prohibition, legal negation, Markdown, deployment
prohibition, time zone, and permission-vs-probability distinction. Its ten
latencies stayed between 0.755s and 1.237s.

Manual Super review found three strong translations, one partial result, five
serious defects, and one timeout. It lost a same-day deadline, removed required
line/bracket formatting, broke a placeholder, emitted a self-correction
monologue despite reasoning being disabled, changed a named legal party,
translated a person's name as `Sea`, invented gender, and timed out after
120.008s on difficulty ten. The 12.536s difficulty-six tail was also a valid
response, not benchmark pacing.

Decision:

- **Ling 3.0 Flash is the one additional candidate.** It is slower than the
  admitted Omni route but materially more accurate automatically and extremely
  consistent in this run. Admit only as a general text fallback; do not use it
  for Help Assistant or control tasks where name/entity fidelity is critical.
- **Reject Nemotron 3 Super.** Its public throughput rank did not translate to
  reliable product-path completion latency or acceptable instruction fidelity.

## Vision benchmark

| Model | Suite | Reliability | Accuracy | All-case median | P95 | Representative-small median |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Nemotron Nano 12B v2 VL | Coordinate | 9/10 | 2/9 hits | 4.314s | 6.546s | 2.119s |
| Nemotron Nano 12B v2 VL | OCR | 10/10 | 86.871%; 5 strict | 6.759s | 15.714s | 3.610s |
| Nemotron 3 Nano Omni 30B A3B | Coordinate | 9/10 | 6/9 hits | 0.650s | 1.417s | — |
| Nemotron 3 Nano Omni 30B A3B | OCR | 9/10 | 94.367%; 5 strict | 0.733s | 1.232s | 0.446s |

The VL model's fourth coordinate request timed out after 120.551s while
parsing the non-streaming response. All nineteen successful responses reported
zero reasoning characters.

Decision: **reject Nemotron Nano 12B v2 VL.** Its advertised OCR specialization
did not beat Omni on accuracy, speed, coordinate grounding, or tail behavior
for this product's image set.

## Raw local evidence

- `target/catalog-benchmark/openrouter-super-ling-20260724/`
- `target/catalog-benchmark/openrouter-nano-vl-20260724/`
