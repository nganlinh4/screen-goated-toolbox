# NVIDIA NIM — provider evaluation, 2026-08-20 (banked, not adopted)

Kept for a later hosted monitoring plan. Nothing here is cataloged, and none of
these figures may enter the catalog: they come from direct calls rather than the
production request path, and are scored by character similarity rather than the
harness's rubric. Only comparisons inside this document are valid.

## Calibration

Measured on the identical metric and corpus so the table below means something:
`gemini-3.5-flash-lite`, our image chain position 1, scores **0.859 text / 0.92s**
and **0.723 OCR / 2.13s**. The same endpoint scores 0.879 under the harness, which
is why cross-referencing these numbers to catalog values is invalid.

A Groq Qwen baseline was attempted and abandoned at 4 of 10 cases when the daily
token budget ran out, so NVIDIA has not been compared against our image leader.

## Reachability

Of 75 chat-capable models advertised by `/v1/models`, **22 answered**;
41 returned 404 and 10 failed to connect. The inventory is not a availability
signal, unlike Groq and Gemini.

## Text, ten corpus cases

| Endpoint | Mean | Median | Completed | Reasoning control |
| --- | ---: | ---: | ---: | --- |
| `openai/gpt-oss-20b` | 0.827 | 0.75s | 10/10 | `effort=low` |
| `mistralai/mistral-nemotron` | 0.825 | 0.76s | 6/10 | `plain` |
| `openai/gpt-oss-120b` | 0.819 | 0.84s | 10/10 | `effort=low` |
| `nvidia/nemotron-3-super-120b-a12b` | 0.803 | 0.56s | 10/10 | `effort=none` |
| `nvidia/nemotron-3-nano-omni-30b-a3b-reasoning` | 0.784 | 0.94s | 10/10 | `effort=none` |
| `poolside/laguna-xs-2.1` | 0.783 | 0.54s | 10/10 | `plain` |
| `nvidia/nemotron-3.5-lightning-30b-a3b` | 0.765 | 0.52s | 10/10 | `effort=none` |
| `nvidia/nemotron-3-nano-30b-a3b` | 0.753 | 0.48s | 10/10 | `effort=none` |
| `nvidia/nemotron-mini-4b-instruct` | 0.686 | 0.69s | 10/10 | `plain` |

Every one scores below the 0.859 baseline, and several are twice as fast. They
would be a speed choice, not a quality one.

`mistralai/mistral-nemotron` completed 6 of 10 and `minimaxai/minimax-m3` none,
the latter rate-limited throughout.

## Vision, ten corpus cases

| Endpoint | Mean | Median | Completed |
| --- | ---: | ---: | ---: |
| `nvidia/nemotron-3-nano-omni-30b-a3b-reasoning` | 0.723 | 1.26s | 10/10 |
| `thinkingmachines/inkling` | 0.670 | 2.45s | 4/10 |
| `nvidia/nemotron-nano-12b-v2-vl` | 0.543 | 38.51s | 5/10 |

`nemotron-3-nano-omni` matches the 0.723 baseline at 1.26s against 2.13s. It is
the only NVIDIA vision endpoint that finished the corpus.

## Reasoning control is per model, not per provider

Nineteen endpoints reach zero reasoning tokens, thirteen of them on a plain
request. The Nemotron family toggles through a system prompt (`/no_think`,
`detailed thinking off`) rather than an OpenAI-style parameter, and `gpt-oss`
requires `low` because it rejects `none`.

An earlier pass applied `reasoning_effort` to every endpoint and recorded the
results as model verdicts. That was wrong in both directions: it rejected
`llama-3.3-nemotron-super-49b-v1.5`, `nvidia-nemotron-nano-9b-v2`, `minimax-m3`,
`llama-3.1-70b` and `mistral-nemotron`, all of which work, and the parameter
itself caused `google/gemma-4-31b-it` to return HTTP 500. Community leaderboards
can carry the same error; nimstats lists a model at 0% that answers 3 of 3 here.

## Risks if this is revisited

Three endpoints changed state within one day: `minimax-m3` working to none,
`mistral-nemotron` failing to healthy to partial, `glm-5.2` healthy to dead. One
corpus pass measures completion, not reliability, and our ordering rule gates on
reliability first.

NVIDIA publishes no `x-ratelimit-*` headers, so the header-driven admission check
in `src/retry_model_chain/budget.rs` would be blind there and fall back to
reacting to 429s.

Wiring touches roughly thirty files carrying a provider arm today, plus six parity
specs and fixtures, plus the Android side.

## If adopted later

Wire the provider, add rows enabled but absent from every priority chain, measure
through the harness, and place them only on that data — the sequence used for
Gemini 3.7 and the OpenRouter candidates.
