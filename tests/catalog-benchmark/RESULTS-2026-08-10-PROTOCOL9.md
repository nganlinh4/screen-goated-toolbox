# Protocol 9 catalog decision — 2026-08-10

Protocol 9 contains 520 round-major attempts across the production text,
coordinate-grounding, and OCR paths. It replaces the implicit coordinate
schema wording with the exact fail-closed JSON contract used by Computer and
Phone Control. The completed logical report merges the base run with two
focused Gemini recoveries. It contains 483 successful attempts and 37 measured
failures; failed provider calls remain part of reliability.

## Text

| Endpoint | Success | Automatic score | Median | P95 | Manual review |
| --- | ---: | ---: | ---: | ---: | --- |
| Cerebras GPT-OSS 120B | 10/10 | 0.830 | 0.368s | 0.651s | 8/10 strict cases; 27/29 rubric criteria |
| Cerebras GLM 4.7 | 10/10 | 0.860 | 0.409s | 1.590s | 8/10 strict cases; 27/29 rubric criteria |
| Groq GPT-OSS 20B | 10/10 | 0.803 | 0.495s | 0.690s | 8/10 strict cases; 26/29 rubric criteria |
| Groq GPT-OSS 120B | 10/10 | 0.824 | 0.549s | 0.719s | 7/10 strict cases; 26/29 rubric criteria |
| Gemini 3.5 Flash Lite | 10/10 | 0.836 | 0.592s | 0.643s | 9/10 strict cases; 28/29 rubric criteria |
| Gemini 3.1 Flash Lite | 10/10 | 0.864 | 0.643s | 1.153s | 9/10 strict cases; 28/29 rubric criteria |
| Gemini 3.5 Flash | 10/10 | 0.852 | 0.803s | 5.371s | Complete after recovery; recovered outputs manually reviewed |
| Gemini Robotics ER1.6 | 10/10 | 0.832 | 1.032s | 1.335s | Complete after recovery; one non-neutral `he/she` rendering |
| Gemini 3 Flash | 10/10 | 0.825 | 1.106s | 1.294s | Complete after recovery; recovered outputs manually reviewed |
| Gemini 3.6 Flash | 10/10 | 0.856 | 1.180s | 1.985s | Complete after recovery; one mixed-language Vietnamese rendering |
| OpenRouter Nemotron Omni | 10/10 | 0.738 | 0.646s | 1.082s | 6/10 strict cases; 25/29 rubric criteria |
| Google GTX | 10/10 | 0.831 | 0.206s | 0.456s | 7/10 strict cases; 26/29 rubric criteria; translation only |
| Taalas Llama 8B | 9/10 | 0.502 | 0.644s | 0.714s | Rejected for repeated wrong-language and semantic failures |

Cerebras GLM remains the user-selected quality-first default. The general
fallback order now favors Cerebras GPT-OSS, Gemini 3.5 Flash Lite, and Groq
GPT-OSS 20B before slower or less accurate endpoints. Automatic similarity is
only supporting evidence; the manual column is authoritative.

Groq Compound completed only 6/10 calls, and some successful responses included
reasoning despite the translation-only instruction. It remains search-specific.
Groq's `llama-3.1-8b-instant` and `llama-3.3-70b-versatile` are removed because
their provider shutdown is scheduled for 2026-08-16.

## General image and OCR

| Endpoint | Success | Mean OCR | Strict OCR | Small-image median | All-case P95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Gemini 3.5 Flash Lite | 10/10 | 0.876 | 70% | 0.882s | 1.560s |
| Gemini 3.1 Flash Lite | 10/10 | 0.890 | 80% | 0.994s | 2.465s |
| Gemini 3.5 Flash | 10/10 | 0.866 | 70% | 1.091s | 1.770s |
| Gemini 3.6 Flash | 10/10 | 0.917 | 80% | 1.414s | 19.431s |
| Gemini Robotics ER1.6 | 10/10 | 0.904 | 80% | 1.595s | 9.344s |
| Gemini 3 Flash | 10/10 | 0.907 | 70% | 1.675s | 2.056s |
| Groq Qwen 3.6 | 10/10 | 0.944 | 80% | 1.037s | 7.521s |
| Gemini Robotics ER2 | 10/10 | 0.892 | 70% | 1.272s | 5.746s |
| Cerebras Gemma 4 31B | 9/10 | 0.901 | 78% | 0.900s | 9.640s |
| OpenRouter Nemotron Omni | 9/10 | 0.914 | 33% | 1.058s | 9.931s |
| Google Gemma 4 31B | 10/10 | 0.893 | 80% | 2.189s | 4.168s |
| Google Gemma 4 26B A4B | 10/10 | 0.866 | 70% | 1.854s | 4.720s |

The image chain keeps Gemini 3.5 Flash Lite first, then Gemini 3.1 Flash Lite.
Qwen remains a fast OCR fallback but its 7.5-second tail and weak strict
grounding prevent promotion into control. Robotics ER2 supplies the next fully
available provider-qualified row before the less reliable Cerebras and
OpenRouter routes.

## Coordinate grounding

| Endpoint | Request success | Accepted accuracy | Representative total |
| --- | ---: | ---: | ---: |
| Gemini Robotics ER2 | 10/10 | 100% | 4.271s |
| Gemini 3.5 Flash Lite | 10/10 | 90% | 2.292s |
| Gemini 3.1 Flash Lite | 10/10 | 80% | 2.405s |
| Gemini 3.5 Flash | 10/10 | 100% | 2.755s |
| Gemini 3 Flash | 10/10 | 90% | 3.367s |
| Gemini 3.6 Flash | 10/10 | 80% | 6.312s |
| Gemini Robotics ER1.6 | 10/10 | 90% | 6.973s |
| Google Gemma 4 31B | 10/10 | 90% | 5.167s |
| Cerebras Gemma 4 31B | 10/10 | 60% | 2.698s |
| Groq Qwen 3.6 | 8/10 | 12.5% of successful calls | 1.660s |
| OpenRouter Nemotron Omni | 0/10 | 0% | — |

Computer and Phone Control use Robotics ER2 first because all ten strict
end-to-end checks passed, with the much faster Gemini 3.5 Flash Lite as the
single feature fallback. The parser remains fail-closed: Omni consistently
omitted required authority-bearing fields, so weakening the contract would
make the benchmark less representative of the product.

## Discovery, lifecycle, and quota notes

Authenticated provider inventories confirmed every currently cataloged
Gemini, Groq, and Cerebras target. The OpenRouter free inventory screen rejected
Laguna XS 2.1 and Nemotron Nano 30B for text quality, and rejected OpenRouter
Gemma 4 31B plus Nemotron 12B VL for availability or strict vision behavior.
No provisional candidate entered the product catalog.

Twenty populated, value-distinct Gemini credentials supplied independent
rotation slots. The first focused pass reran all 75 unavailable Gemini cells
and recovered 52. A second pass recovered the remaining four short-window 429s
and one timeout. The final 520-cell logical report contains no Gemini quota
failure.

The 18 remaining Gemini failures are product evidence rather than unfinished
quota work: twelve `gemini-2.5-flash-lite` calls returned 404 because the model
is unavailable to new projects, and six Gemini 2.5 Live coordinate responses
failed the production grounding or verification contract. [Google's published
lifecycle](https://ai.google.dev/gemini-api/docs/deprecations) also schedules
Gemini 2.5 Flash-Lite shutdown for 2026-10-16 and recommends Gemini 3.1
Flash-Lite. The built-in 2.5 Flash-Lite text, vision, and audio rows are
therefore disabled, and its vision retry entry is removed.
