> Superseded by [`RESULTS-2026-08-19-PROTOCOL10.md`](RESULTS-2026-08-19-PROTOCOL10.md).
> The OCR rerun this record required has since run; its chain orders, latencies and
> the decision to keep Gemini 3.7 out have all been replaced by measurement.

# Catalog decision follow-up — 2026-08-19

No new suite ran on this date. Every change here rests on the registered
[`RESULTS-2026-08-18-PROTOCOL9.md`](RESULTS-2026-08-18-PROTOCOL9.md) run plus targeted
live probes, and supersedes that record wherever the two disagree.

## Groq Qwen 3.6 leads the image chain

The protocol-9 record deferred this, and named the reason: promoting Qwen also moves
`default_image_model_id`. The deferral was resolved rather than the evidence changing.
Qwen was the fastest vision endpoint measured at 0.739s and the most accurate at 0.940
mean OCR with 90% strict, and it leads both the chain and the default now.

The argument for keeping it third was that its 8K tokens-per-minute ceiling made it a
poor first contact. That argument was wrong, because it ignored machinery this repo
already had. A rejection costs a median 588ms rather than a timeout, and the first one
opens a cooldown sized to the window the provider reports, after which the endpoint is
skipped at no cost until it reopens. Sparse interactive use never reaches the ceiling;
a burst pays one short call and falls through cleanly.

## Image chain reordered on merit

Quota no longer demotes an endpoint. Twenty rotated Gemini credentials make a 20 RPD
allowance far less binding than a single key would, and reliability remains the only
hard gate.

| # | Endpoint | Small-image median | Mean OCR | Strict |
| ---: | --- | ---: | ---: | ---: |
| 0 | Groq Qwen 3.6 | 0.739s | 0.940 | 90% |
| 1 | Gemini 3 Flash | 1.748s | 0.939 | 90% |
| 2 | Gemini 3.1 Flash Lite | 1.432s | 0.888 | 80% |
| 3 | Gemini 3.5 Flash Lite | 1.234s | 0.871 | 60% |
| 4 | Gemini Robotics ER1.6 | 2.863s | 0.963 | 90% |
| 5 | Gemma 4 31B | 2.430s | 0.898 | 80% |
| 6 | OpenRouter dots-3 Note | 2.666s | 0.894 | 70% |
| 7 | Gemini Robotics ER2 | 2.694s | 0.879 | 70% |
| 8 | Gemma 4 26B A4B | 3.021s | 0.883 | 70% |

Gemini 3 Flash moves from last to second: it essentially ties the leader on accuracy and
was at the bottom only because it allows 20 requests a day. Robotics ER1.6 enters the
chain for the first time while holding the highest mean OCR in the catalog.

In the text chain the OpenRouter row moves to the tail on 5/10 reliability, which is the
hard gate rather than a quota judgement.

## Qwen OCR repetition

Asked for bare text, Qwen deterministically appended a re-tokenized repetition of what it
had just emitted: `Điều khiển máy tính` returned as `Điều khiển máy tính\nĐiều khi\nển máy
tính`, splitting a syllable. At temperature 0 the wrong answer was its highest-probability
completion, reproducing four times in four; removing `presence_penalty` made it worse, and
1x, 2x and 4x upscales returned byte-identical output. The same defect is reported upstream
against Qwen3-VL on vLLM, Ollama and LM Studio (QwenLM/Qwen3-VL#1611).

Constraining the reply to a JSON object fixes it, three of three including greedy decoding.
Groq documents JSON object mode as the supported path for models without strict structured
outputs and ships a vision example using it. Plain-text vision callers on `json-object`
endpoints now send a `{"text": ...}` envelope and unwrap it, non-streaming only, failing
open when the reply is not that object.

This changed OCR request semantics, so `benchmark_protocol_version` is 10 and **every OCR
row needs a fresh run before the next catalog performance update**. Text and coordinate
rows are unaffected. The stale rows understate Qwen rather than flatter it, since they were
measured while the repetition was still truncating scores.

## Uniform output ceiling

The 512-token reserve now applies to every ordinary vision endpoint instead of Qwen alone.
Image behaviour no longer changes with whichever endpoint a chain reaches, and because the
reserve is charged whether or not it is used, a single call is bounded against a
per-minute budget on every provider rather than only the one that publishes headers.

## Preset pins removed

`PRESET_IMAGE_ACCURATE_MODEL_ID` and `PRESET_IMAGE_TRANSLATE_VISION_MODEL_ID` are deleted.
Neither was ever an independent choice: the accurate pin held the default image model's
value on every commit since it was introduced, and the translate pin last differed before
the Cerebras removal. Their only real effect appeared when the default moved and three
presets silently kept the old model. Every ordinary image preset now names the default
directly; the QR scanner keeps its dedicated non-LLM service.

## Rate-limit handling

Cooldowns follow the window a provider reports rather than a flat five minutes. Groq
answers a per-minute rejection with `retry-after: 22`, and a per-day rejection with
`Please try again in 18m50.112s`; Gemini publishes no rate headers but ends its
RESOURCE_EXHAUSTED body with `Please retry in 32.814072061s`. The old constant was wrong
in both directions, wasting a healthy endpoint on short windows and re-probing an
exhausted one every five minutes.

Groq's daily token budget replenishes continuously rather than resetting at a boundary:
observed usage fell from 199,568 to 197,747 without traffic, and a request 186 tokens
short was quoted 81s, which matches 2.3 tokens per second against a 200,000/day limit.
The reported wait is therefore exact, not indicative.

An endpoint is also skipped before the call when its reported balance cannot cover even
the cheapest request it accepts. The floor is measured, not derived: a sweep of eleven
aspect ratios found Groq billing by shape rather than size and not monotonically, with
1024x512 at 770 tokens, 1024x341 at 1026 and 1024x682 at 1794, while resolution alone
changed nothing. Qwen's published `smart_resize` predicts 64-256 tokens for the same
inputs and cannot be used for budgeting here. Only the per-minute window is visible;
Groq publishes no per-day token header, so daily exhaustion is still caught by the
cooldown after one rejected call.

## Catalog additions

Groq Qwen 3.6, Gemini Robotics ER2 and OpenRouter dots-3 Note were vision-only while being
text-capable; all three returned correct translations from a plain text prompt when checked
directly. Each now has a Text row so it can be selected. Their latency comes from a small
dated probe rather than the suite, and none joins a priority chain until benchmarked.

Groq Qwen 3.6 is renamed from `G Thất thường` to `G Tốt`. The old name was earned by a
7.5-second tail and weak strict grounding that this run reverses.

## Benchmark credentials

Groq now rotates indexed credentials like Gemini and OpenRouter; it had been the only
provider held as a single key, and its call sites read the field directly rather than the
pool. A second account was confirmed to carry an independent allowance rather than sharing
one: draining one key to 5,187 remaining tokens and 997 requests left the other reporting
7,920 and 999. Rotation remains benchmark-only.
