# Catalog benchmark decision — 2026-07-26, protocol 6

This is the current catalog decision record. It replaces earlier benchmark
decisions rather than averaging with them.

## Run

- Complete logical report:
  `target/catalog-benchmark/runs/20260726-protocol6-all-r1-complete`
- Attempts: 490
- Successful model calls: 480
- Final quota/auth/provider errors: 0
- Structural failures retained in reliability: 10
- Selection policy: newest complete protocol-compatible row for each model,
  suite, API endpoint, and reasoning policy
- Text timing: median full-answer completion across ten levels
- Vision timing: median full-answer completion over successful OCR inputs at or
  below 1024 px longest edge
- Coordinate timing: two production model calls, locate plus fresh marked-crop
  verification

Focused recovery calls replaced failed quota cells before the report was
registered. The recovery fragments are not separate benchmark results.

## General vision evidence

| Model | Success | Small OCR median | OCR score | OCR strict | Coordinate success | Coordinate strict |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Groq Qwen 3.6 | 10/10 | 0.935s | 0.980 | 0.80 | 10/10 | 0.00 |
| Gemini 3.5 Flash Lite | 10/10 | 0.976s | 0.978 | 0.80 | 9/10 | 1.00 |
| OpenRouter Nemotron | 10/10 | 1.135s | 0.938 | 0.30 | 10/10 | 0.70 |
| Gemini 3.1 Flash Lite | 10/10 | 1.473s | 0.907 | 0.70 | 10/10 | 0.90 |
| Gemini 3.6 Flash | 10/10 | 1.577s | 0.919 | 0.80 | 10/10 | 1.00 |
| Gemini Robotics | 10/10 | 1.711s | 0.995 | 0.90 | 10/10 | 1.00 |
| Cerebras Gemma 4 | 10/10 | 1.758s | 0.928 | 0.80 | 10/10 | 0.60 |
| Gemini 2.5 Flash Lite | 10/10 | 2.103s | 0.843 | 0.40 | 10/10 | 0.50 |
| Google Gemma 4 31B | 10/10 | 2.264s | 0.917 | 0.80 | 10/10 | 0.90 |
| Gemini Live 3.1 | 10/10 | 3.333s | 0.789 | 0.50 | 9/10 | 0.00 |
| Gemini 3 Flash | 10/10 | 3.397s | 0.984 | 0.90 | 10/10 | 1.00 |
| Google Gemma 4 26B | 10/10 | 4.236s | 0.906 | 0.70 | 10/10 | 0.00 |
| Gemini Live 2.5 | 9/10 | 4.307s | 0.642 | 0.11 | 3/10 | 0.00 |
| Gemini 3.5 Flash | 10/10 | 5.788s | 0.916 | 0.70 | 10/10 | 1.00 |

The coordinate strict score is calculated over successful structured responses.
Live 2.5 and Live 3.1 did not produce the minimum four representative
coordinate samples needed for a catalog-ready control latency.

## Text evidence

| Model | Median | Automatic score | Catalog role |
| --- | ---: | ---: | --- |
| Groq Llama 3.3 70B | 0.344s | 0.814 | General chain |
| Cerebras GPT-OSS 120B | 0.347s | 0.793 | Fast fallback |
| Cerebras GLM 4.7 | 0.414s | 0.825 | General default |
| OpenRouter Nemotron | 0.493s | 0.759 | Excluded from general chain |
| Groq GPT-OSS 120B | 0.601s | 0.799 | Fast fallback |
| Gemini 3.5 Flash Lite | 0.624s | 0.878 | Strong general/preset model |
| Gemini 3.1 Flash Lite | 0.632s | 0.857 | General fallback |
| Taalas Llama 3.1 8B | 0.646s | 0.565 | Excluded from general chain |
| Google GTX | 0.705s | 0.828 | Translation specialist |
| Gemini 2.5 Flash Lite | 0.708s | 0.914 | High-quality limited fallback |
| OpenRouter Ling 3 Flash | 0.833s | 0.837 | OpenRouter fallback |
| Gemini 3.6 Flash | 0.949s | 0.837 | Selectable, redundant in chain |
| Gemini Robotics | 1.001s | 0.866 | Vision accuracy specialist |
| Gemini 3 Flash | 1.006s | 0.869 | Strong late fallback |
| Groq Compound Mini | 1.083s | 0.812 | Search specialist/default |
| Google Gemma 4 26B | 1.275s | 0.862 | Selectable, redundant in chain |
| Google Gemma 4 31B | 1.366s | 0.884 | High-quality late fallback |
| Groq Compound | 1.981s | 0.596 | Selectable search alternative |
| Gemini Live 3.1 | 2.715s | 0.852 | Live specialist |
| Gemini Live 2.5 | 3.284s | 0.757 | Older Live specialist |
| Gemini 3.5 Flash | 3.537s | 0.846 | Selectable accuracy alternative |

Human review remains authoritative for translation. OpenRouter Nemotron had a
severe semantic failure on the hardest recovery-message case, and Taalas also
failed that case badly; neither belongs in the general text chain despite low
latency. Compound models remain search-specific.

## Applied recommendation set

### Vision

- Broad default and Ask Image: `google-gemini-3-5-flash-lite-vision`
- Fast image-translation family:
  `groq-qwen-3-6-27b-vision`
- Accuracy-labeled OCR/table/fact-check family:
  `google-gemini-robotics-er-1-6-vision`
- Help Assistant:
  Gemini 3.5 Flash Lite, then Gemini 3.1 Flash Lite
- Computer/Phone Control:
  Gemini 3.5 Flash Lite, then Gemini 3.1 Flash Lite

General image retry order:

1. Gemini 3.5 Flash Lite
2. Groq Qwen 3.6
3. OpenRouter Nemotron
4. Gemini 3.1 Flash Lite
5. Gemini 3.6 Flash
6. Gemini Robotics
7. Cerebras Gemma 4
8. Google Gemma 4 31B
9. Gemini 3 Flash
10. Google Gemma 4 26B

Qwen leads only image-translation presets. Its excellent OCR speed does not
justify a broad default because it scored 0/10 on coordinate correctness and
showed transient serving instability during the base run. Gemini 3.5 Lite costs
only 41 ms more on representative OCR and is much broader.

### Text

- General default: `cerebras-zai-glm-4-7-text`
- Fast text arena: `groq-llama-3-3-70b-text`
- Game/constraint-heavy text preset:
  `google-gemini-3-5-flash-lite-text`
- Search: `groq-compound-mini-search`

General text retry order:

1. Cerebras GLM 4.7
2. Groq Llama 3.3 70B
3. Gemini 3.5 Flash Lite
4. OpenRouter Ling 3 Flash
5. Cerebras GPT-OSS 120B
6. Groq GPT-OSS 120B
7. Gemini 3.1 Flash Lite
8. Gemini 2.5 Flash Lite
9. Gemini 3 Flash
10. Google Gemma 4 31B

The first four provide a fast usable model from each recommended hosted
provider. Later positions retain fast alternatives and high-quality quota
fallbacks. OpenRouter Nemotron is intentionally absent.

### Audio and other feature defaults

- Continuous writing moves from Live 2.5 to Live 3.1. The new benchmark is only
  proxy evidence for Live endpoint quality, but Live 3.1 was faster and far
  more structurally reliable while preserving input-transcription semantics.
- Direct finite audio translation moves from Gemini 2.5 Flash Lite to Gemini
  3.5 Flash Lite.
- Groq Whisper Large remains the ordinary transcription default. This benchmark
  did not measure audio recognition, so it cannot justify replacing it with
  Whisper Turbo.
- The existing Gemini 3.5 Live Translate realtime-session default, Google GTX
  arena slot, Parakeet offline transcription, TTS model, QR scanner, and local
  ASR choices remain unchanged because their relevant task was not measured.

### Presentation

- Gemini 3 Flash becomes `GG Toàn diện` / `GG Versatile` / `GG 다재다능`,
  intelligence tier 6.
- Gemini Robotics becomes `GG Chuẩn` / `GG Precise` / `GG 정밀`,
  intelligence tier 6.
- Groq Qwen moves to intelligence tier 4; its `G Thất thường` name continues to
  carry the observed reliability warning.
- OpenRouter Nemotron becomes `O Nhanh` / `O Fast` / `O 빠름`; “Very fast” was
  no longer accurate for general vision.
- Every benchmarked text and vision row receives its protocol-6 latest-run
  latency and source identifier.

### Non-preset consumers

- New processing blocks and new Windows/Android graph nodes now use the
  catalog-owned modality defaults.
- Live Translate's follow-up text block uses the catalog text default.
- Result refinement now resolves the first enabled, credentialed model from the
  configured text priority chain instead of carrying a private Gemini 2.5 Lite
  preference.
