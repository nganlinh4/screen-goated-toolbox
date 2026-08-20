# Catalog benchmark — 2026-08-20 (protocol 11), OCR only

The OCR re-run required by the protocol-10 amendment. 150 attempts over 15 rows.
Text, coordinate and localization rows are unchanged and still rest on the
[2026-08-19 run](RESULTS-2026-08-19-PROTOCOL10.md); only the OCR request shape had
changed, twice, through the removal of the JSON envelope and the addition of the
repetition salvage guard.

## Qwen 3.6 keeps the image lead on a real number

The voided 0.989 is replaced by **0.962 mean at 80% strict, 100% reliability and a
1.23s median**, measured through the request the product now sends. It is the only
endpoint combining full reliability with high accuracy at low latency, and its
representative small-image median improves to 642ms.

## Reliability this run is not usable for ordering

Eight endpoints show exactly one failure each, and all eight fall in the same round
on the same case, with Gemini answering `429 RESOURCE_EXHAUSTED`. That is one
shared quota exhaustion rather than eight independent weaknesses, so the 90% figures
below are an artifact. **No chain was reordered on this run.** Latency and accuracy
come from successful attempts and are unaffected.

| Endpoint | Reliability | Median | P95 | Mean | Strict |
| --- | ---: | ---: | ---: | ---: | ---: |
| `groq-qwen-3-6-27b-vision` | 100% | 1.23s | 4.51s | 0.962 | 80% |
| `google-gemini-3-5-flash-lite-vision` | 90% | 1.35s | 2.03s | 0.879 | 78% |
| `google-gemini-3-5-flash-vision` | 90% | 1.46s | 9.25s | 0.855 | 78% |
| `google-gemini-robotics-er-2-vision` | 90% | 3.72s | 5.97s | 0.887 | 78% |
| `google-gemini-3-flash-vision` | 90% | 1.83s | 7.25s | 0.896 | 78% |
| `google-gemini-3-7-flash-vision` | 90% | 3.84s | 24.62s | 0.919 | 89% |
| `google-gemini-robotics-er-1-6-vision` | 90% | 3.73s | 4.39s | 0.897 | 78% |
| `google-gemma-4-26b-a4b-vision` | 90% | 4.07s | 12.86s | 0.862 | 67% |
| `google-gemma-4-31b-vision` | 100% | 3.07s | 12.58s | 0.885 | 80% |
| `openrouter-dots-3-note-vision` | 100% | 3.81s | 7.97s | 0.879 | 60% |
| `google-gemini-3-1-flash-lite-vision` | 100% | 3.33s | 19.23s | 0.891 | 80% |
| `google-gemini-3-6-flash-vision` | 90% | 11.01s | 30.25s | 0.846 | 67% |

## The corpus now provokes what it measures

Two OCR cases were replaced, both chosen because the protocol-10 run showed they
discriminated nothing. Handwriting at difficulty 10 scored 0.991 mean across
fourteen endpoints with a 0.039 spread, and perspective diacritics at difficulty 2
scored 0.996 with a 0.050 spread. Every model passed both, so neither slot measured
anything.

They are replaced by a terminal directory listing and a pair of file names differing
only in their timestamp. Both discriminate sharply, at 0.383 and 0.923 spread.

The directory listing did not do what it was chosen for, and that is the more useful
finding. Probed raw, outside the salvage guard, it never repeated in four attempts;
it truncates instead, every reply stopping mid-identifier at the 512-token ceiling
because the reference runs to about 1,100 characters. The repetition defect needs a
reply short enough to finish, after which the model carries on generating. A long
transcription reaches the ceiling first. The short case at difficulty 2 is therefore
the one that carries the defect, and it reproduces there.

The suite keeps exactly ten cases at difficulties one through ten and the required
mix of screen crops and original files.

## NVIDIA NIM, measured fairly

The 2026-08-19 survey timed these endpoints with reasoning left on and concluded the
provider offered only failover depth. That conclusion was wrong. With reasoning
disabled the same endpoints are competitive with the fastest text rows in the
catalog.

| Endpoint | Thinking on | Thinking off |
| --- | ---: | ---: |
| `nvidia/nemotron-3-super-120b-a12b` | 1.25s | **0.34s** |
| `nvidia/nemotron-3-nano-30b-a3b` | 1.59s | **0.33s** |
| `nvidia/nemotron-3.5-lightning-30b-a3b` | 4.24s, reasoning in content | **0.58s**, correct |
| `openai/gpt-oss-120b` | 1.66s | 1.08s, requires `low` |
| `z-ai/glm-5.2` | 1.71s | unreliable, connection failures and 429 |

`nemotron-3.5-lightning` was rejected on 2026-08-19 for emitting its reasoning as
ordinary content. With `reasoning_effort: none` it answers cleanly, so that
rejection is withdrawn.

The provider accepts `reasoning_effort: none`, which is the existing `openai-none`
policy, so cataloging it would need no new reasoning plumbing. It still publishes no
rate-limit headers, leaving the predictor blind, still 404s on a large share of the
models it lists, and its vision endpoints remain unusable. Nothing is cataloged from
it yet; the case for the wiring is now speed rather than depth.
