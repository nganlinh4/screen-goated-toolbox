# Text candidate decision — 2026-08-06

Protocol 6 exercised the production text translation path for ten round-major
difficulty levels per candidate. The complete local report contained 30 text
attempts; every request succeeded. Automatic similarity remained a review aid,
and the final decision included manual rubric inspection.

| Endpoint | Median | P95 | Automatic score | Decision |
| --- | ---: | ---: | ---: | --- |
| `openai/gpt-oss-20b` on Groq | 0.561s | 0.800s | 79.0% | Add as a strong fast fallback |
| `llama-3.1-8b-instant` on Groq | 0.258s | 0.541s | 76.6% | Add as the fastest lightweight fallback |
| `nvidia/nemotron-3-nano-30b-a3b:free` on OpenRouter | 0.595s | 0.786s | 71.5% | Reject after substantive semantic errors |

GPT-OSS 20B used Groq's lowest supported reasoning effort. Llama 8B has no
reasoning control and trades a small amount of automatic score for the lowest
observed latency. Both remain in the authored retry order as fast fallbacks.
