# Protocol 7 catalog decision — 2026-08-06

Protocol 7 replaced line-based coordinate records with strict JSON point
collections, made Google vision requests image-first, and exercised the same
schema-aware Rust request path used by Computer Control. The complete vision
report contains 300 attempts; the text report contains 230 attempts.

## Grounding

| Endpoint | Success | Accepted accuracy | Median total |
| --- | ---: | ---: | ---: |
| Gemini 3.5 Flash Lite | 10/10 | 80% | 2.77s |
| Gemini 3.1 Flash Lite | 10/10 | 80% | 5.20s |
| Google Gemma 4 31B | 10/10 | 90% | 7.00s |
| Cerebras Gemma 4 31B | 6/10 | 67% | 6.14s |
| Gemini Robotics ER2 | 6/10 | 100% | 28.80s |

Robotics ER2's previous structural failures were transport/parser failures: it
now returns valid coordinates, but its latency and quota reliability do not
justify priority promotion. Computer and Phone Control therefore remain Gemini
3.5 Flash Lite followed by Gemini 3.1 Flash Lite.

## General image

Gemini 3.5 Flash Lite remains the default: 10/10 OCR reliability, 93.1% mean
similarity, and a 1.05s representative-small-image median. The general retry
chain next favors Groq Qwen and OpenRouter Nemotron for their fast, fully
available OCR paths, then Gemini 3.1 Flash Lite. Cerebras Gemma moves lower
because overload reduced both OCR and coordinate reliability.

## Text

Human review retains Cerebras GLM as the general quality-first default. Groq
GPT-OSS 20B becomes its fast fallback and the fast-arena seed: 10/10 reliability,
81.9% automatic similarity, and a 0.49s median. Cerebras GLM completed only 4/10
calls in this run, so its availability risk remains explicit without allowing
the automatic benchmark to displace the preferred answer quality.

Automatic translation similarity remains a review aid. The priority decision
also considers exact-case output, availability, provider diversity, and latency
tails; it does not rank solely by the mean score.
