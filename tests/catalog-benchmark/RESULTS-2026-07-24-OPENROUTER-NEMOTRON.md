# OpenRouter Nemotron admission — 2026-07-24

> Historical record only. The three-run rolling policy described below was
> retired on 2026-07-26. Current decisions use
> `RESULTS-2026-07-26-PROTOCOL6.md` and the latest-complete-run policy.

This record documents the provisional admission of
`nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free`. It does not replace the
rolling three-run policy. The user explicitly accepted quota risk and requested
catalog and priority integration after reviewing the first complete benchmark.
The `single-run-provisional` performance source makes that exception visible
until three comparable runs replace it.

## Production policy comparison

The same text prompt was sent once with OpenRouter's default reasoning and once
with `reasoning: { "effort": "none" }`.

| Policy | Completion | Reasoning characters | Reasoning tokens |
| --- | ---: | ---: | ---: |
| Provider default | 19.483s | 834 | 229 |
| Effort `none` | 1.501s | 0 | 0 |

All 28 successful responses in the complete reasoning-off run reported zero
reasoning. Catalog and production transports therefore use the nested
OpenRouter field. `include_reasoning: false` or `exclude: true` would only hide
reasoning output and are not equivalent.

The live endpoint lookup on 2026-07-24 exposed one NVIDIA endpoint for this
free model. A `provider.sort` preference therefore cannot improve this route;
disabling ordinary reasoning is the material transport optimization.

## Complete ten-level reasoning-off run

| Suite | Reliability | Accuracy | Median completion | P95 |
| --- | ---: | ---: | ---: | ---: |
| Text translation | 10/10 | 75.899% automatic; 7 strong, 1 partial, 2 serious on review | 0.437s | 0.821s |
| Coordinate grounding | 9/10 | 6/9 strict box hits | 0.650s | 1.417s |
| OCR | 9/10 | 94.367% mean similarity; 5 strict | 0.733s | 1.232s |

The representative OCR cohort with an effective longest edge no greater than
1024 px completed in 468, 436, 446, and 1,087 ms; its median was 446 ms. The
fifth representative case hit OpenRouter's 20-requests/minute gate.

The two failed cells were benchmark-pacing artifacts, not model responses. The
helper now enforces a 3.1-second OpenRouter minimum for future runs. The 90%
coordinate/OCR reliability above remains the literal evidence from this run,
but must not be interpreted as a model-availability estimate.

Manual translation review found two important semantic failures: one output
inverted “do not delete” into an instruction to delete, and another translated
a failed backup as spoiled food while inventing a gender. The model therefore
receives intelligence tier 4 and is not admitted to Help Assistant or
Computer/Phone Control. It enters the general text and image fallback chains
behind their existing defaults, where provider preflight, cooldown, and
fallback behavior contain quota or transport failures.

## Raw local evidence

- `target/catalog-benchmark/openrouter-nemotron-omni-20260724-155633/`
- `target/catalog-benchmark/openrouter-nemotron-omni-reasoning-none-20260724-163916/`

These raw outputs are local and Git-ignored. Run two additional independent
protocol-5 benches after quota reset, register all three comparable runs, and
replace the provisional latency with the rolling median-of-run-medians.
