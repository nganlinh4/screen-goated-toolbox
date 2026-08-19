# Catalog benchmark — 2026-08-19 (protocol 10)

Full-catalog run: 520 attempts over 44 rows in 2h09m, plus two merged recovery
passes covering rows added after the main run began. Merged left to right into one
logical run; no fragment is registered independently. This is the first complete
run under protocol 10 and it discharges the OCR rerun that
[`RESULTS-2026-08-19-FOLLOWUP.md`](RESULTS-2026-08-19-FOLLOWUP.md) required.

## The JSON envelope fix is confirmed

Qwen 3.6 was the reason protocol moved to 10, and the rerun settles it. Mean OCR
rose from 0.940 to **0.989** at 100% reliability, the best accuracy of any endpoint
that finishes every attempt. It keeps the image lead and the default image model.
Its measured small-image median moved from 0.739s to 1.053s; the earlier figure was
taken under the request shape that was producing the repetition defect, so the two
are not comparable and the new one supersedes it rather than contradicting it.

## Gemini 3.7 is cataloged

The protocol-9 record declined this endpoint, and that decision is reversed. The
stated blocker was a `gemini-low` reasoning policy, since the model rejects
`MINIMAL` with HTTP 400 and every other cataloged Gemini row uses it. That work was
described as threading through the Rust mapping; in fact `GeminiLevel` was already
generic over the level string, so Rust needed no change and the policy cost one line
in each of four generators plus one Kotlin enum arm. The blocker was overstated, and
overstating it cost a full discovery cycle re-deriving a finding the record already
held.

On measurement the endpoint earns a place: 100% text reliability at 2.46s, and the
only perfect OCR in the catalog at 0.998 mean with 100% strict. Its weakness is
capacity, not quality — 70% OCR completion and a 49.5s P95 — so it sits mid-chain
where the cooldown machinery absorbs a rejection and the accuracy is still reachable.
Launch-period overload is a placement question, not an admission one.

## Endpoint removed

`openrouter-nemotron-3-nano-omni-30b-a3b` measured 10% on text and 0% on both OCR
and coordinate. It was already the standing monitoring item at 5/10; it is removed
rather than demoted, together with its profiles and endpoint metadata.

## Cross-provider rows

Duplicate-endpoint rejection was dropped from discovery: the same model behind a
second provider is an independent quota pool and failure domain. Measured, the
result is mixed and belongs per model rather than per provider. `gemma-4-31b` via
OpenRouter returned HTTP 429 on 40% of text and every vision attempt and was
dropped. `gemma-4-26b-a4b` holds at 100% text and 90% OCR but at 3.46s and 12.11s
against 1.66s and 4.15s on the direct Google path. `gpt-oss-20b` reaches 90% at
13.83s against 0.53s on Groq. Both survive at the chain tail only.

`gpt-oss-20b` first measured 0% with `Reasoning is mandatory for this endpoint and
cannot be disabled`. That was a catalog error, not a dead endpoint: the row carried
`openai-none`. It is `provider-managed` now.

## Chains

### Text

| # | Endpoint | Reliability | Median | P95 | Mean |
| ---: | --- | ---: | ---: | ---: | ---: |
| 0 | `groq-qwen-3-6-27b-text` | 90% | 0.27s | 0.42s | 0.836 |
| 1 | `groq-gpt-oss-120b-text` | 100% | 0.62s | 1.00s | 0.818 |
| 2 | `google-gemini-3-5-flash-lite-text` | 100% | 0.90s | 1.13s | 0.859 |
| 3 | `google-gemini-robotics-er-2-text` | 100% | 1.10s | 1.19s | 0.888 |
| 4 | `openrouter-nemotron-3-super-120b-text` | 100% | 0.91s | 3.01s | 0.768 |
| 5 | `google-gemini-3-flash-text` | 100% | 1.36s | 1.43s | 0.845 |
| 6 | `google-gemini-3-7-flash-text` | 100% | 2.46s | 11.44s | 0.856 |
| 7 | `google-gemma-4-26b-a4b-text` | 100% | 1.66s | 18.39s | 0.854 |
| 8 | `groq-gpt-oss-20b-text` | 100% | 0.53s | 0.63s | 0.778 |
| 9 | `google-gemma-4-31b-text` | 100% | 10.17s | 26.30s | 0.846 |

### Image

| # | Endpoint | Reliability | Median | P95 | Mean | Strict |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 0 | `groq-qwen-3-6-27b-vision` | 100% | 1.72s | 5.37s | 0.989 | 80% |
| 1 | `google-gemini-3-5-flash-lite-vision` | 100% | 1.87s | 6.47s | 0.881 | 70% |
| 2 | `google-gemini-3-5-flash-vision` | 100% | 2.12s | 11.19s | 0.869 | 80% |
| 3 | `google-gemini-robotics-er-2-vision` | 100% | 2.25s | 3.81s | 0.896 | 80% |
| 4 | `google-gemini-3-flash-vision` | 100% | 2.30s | 3.19s | 0.900 | 70% |
| 5 | `google-gemini-3-7-flash-vision` | 70% | 2.81s | 49.48s | 0.998 | 100% |
| 6 | `google-gemini-robotics-er-1-6-vision` | 100% | 3.24s | 4.86s | 0.886 | 70% |
| 7 | `google-gemma-4-26b-a4b-vision` | 100% | 4.15s | 7.62s | 0.887 | 80% |
| 8 | `google-gemma-4-31b-vision` | 100% | 4.34s | 33.55s | 0.879 | 70% |
| 9 | `openrouter-dots-3-note-vision` | 100% | 5.28s | 8.63s | 0.897 | 70% |
| 10 | `google-gemini-3-1-flash-lite-vision` | 100% | 6.98s | 15.58s | 0.890 | 80% |
| 11 | `google-gemini-3-6-flash-vision` | 100% | 11.02s | 27.20s | 0.917 | 80% |

The text leader is the fastest measured endpoint at 0.27s and the first chain lead
under 100% reliability, accepted deliberately: a rejection costs a single fast call
before the 100%-reliable Groq row behind it answers. Leading the chain also moves
`default_text_model_id`, the same coupling the image default has.

`groq-gpt-oss-20b-text` is fast at 0.53s but holds the lowest translation quality of
any enabled text row, so it sits at 8 rather than forward.

## Coordinate and feature chains

Coordinate is unchanged in membership and strong: `gemini-3-flash` 4.58s,
`gemini-3.5-flash` 4.74s, `robotics-er-2` 5.80s and `robotics-er-1.6` 6.50s all
reach 1.000 mean at 100% strict. Grounding promotes `gemini-3-flash` over
`robotics-er-2` on equal perfect accuracy and a faster median. Help assistant
replaces `gemini-3.1-flash-lite` with `gemini-3.5-flash` on 2.12s against 6.98s.

`groq-qwen-3-6-27b-vision` measured 20% on coordinate against 100% on OCR. The
grounding chain stays separate from the general image chain for exactly this reason:
the catalog's best OCR endpoint is among its worst at pointing.

## Provider discovery

No cataloged endpoint disappeared upstream. `gemini-omni-flash-preview`,
`gemini-3.1-pro-preview` and `gemini-2.5-computer-use-preview` return HTTP 429
immediately on four independent unused keys, which is free-tier quota of zero rather
than exhaustion, and they are excluded under the no-billing requirement.
`nemotron-3.5-lightning` emits its reasoning as ordinary content on both OpenRouter
and NVIDIA, so the defect is the model's rather than a route's, and it is rejected.

NVIDIA NIM was surveyed but not cataloged. It is OpenAI-compatible at
`integrate.api.nvidia.com/v1`, allows up to 40 RPM, publishes no rate-limit headers
at all, and lists 102 models of which a large share return 404, 500 or a connection
failure on inference. Four text endpoints answer correctly between 1.25s and 1.71s,
`glm-5.2` among them, which is rate-limited to failure on OpenRouter. Its vision
endpoints are unusable, the best accurate one taking 23.6s. `riva-translate-4b-v2`
answers in 0.5–0.7s but silently returns English for any non-English target, which
is a wrong-language success rather than an error and needs conditional routing
before it could be used.
