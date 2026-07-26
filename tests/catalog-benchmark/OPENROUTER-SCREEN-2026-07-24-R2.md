# OpenRouter free-model screen — 2026-07-24 R2

> Historical record only. Any rolling-history language below predates the
> latest-complete-run policy adopted on 2026-07-26.

This second discovery pass started from the live free-model list after
admitting Ling 3.0 Flash. Content-safety and audio-generation endpoints,
provider duplicates already in the catalog, the random free router, previously
rejected models, retiring Laguna M.1, smaller duplicate GPT-OSS 20B, and
code-only routes without a relevant product role were excluded.

Every screened request used the production OpenRouter message shape and nested
`reasoning: { "effort": "none" }`. These direct non-streaming requests are
admission screens, not catalog-ready Rust benchmark history.

## Two-case adversarial screen

The same gender/entity case and structured-constraint case were sent to four
models.

| Model | Completion evidence | Reasoning | Review | Decision |
| --- | --- | ---: | --- | --- |
| Nemotron 3 Nano 30B A3B | 1.099s, 1.389s | 0 tokens | Fast, but dropped Ekin's role, invented gender, and changed backup to file | Reject |
| Poolside Laguna S 2.1 | 8.682s; second response had no choices array | 0 in success | Coding-specialized, invented gender, slow/unreliable | Reject |
| Nemotron Nano 9B v2 | 22.770s, 15.991s | 686, 677 tokens | Ignored reasoning-off, corrupted entities, reversed the modified-file constraint | Reject |
| Nemotron 3 Ultra 550B A55B | 0.773s, 0.930s | 0 tokens | Strong structured output; invented gender and changed failed backup to corrupted | Escalated |

## Ten-case Ultra transport screen

Cargo's shared artifact directory was held by unrelated concurrent work, and
an isolated build remained blocked on Cargo's global package cache. No Rust
benchmark attempt was produced. The remaining seven levels were therefore
sent through the exact OpenRouter request body directly and combined with the
three existing Ultra screens. This result must not be registered in rolling
benchmark history.

| Reliability | Median | Range | Manual review |
| ---: | ---: | ---: | --- |
| 10/10 | 0.985s | 0.773–11.491s | 6 strong, 2 partial, 2 serious |

Ultra preserved numbers, negation, placeholders, legal scope,
counterfactual meaning, Markdown, deployment prohibition, time zone, and
permission-versus-probability. It weakened a warning and lost bracketed UI
labels, translated a Korean idiom literally as opening a lid, used awkward
Vietnamese, invented gender, and changed a failed backup into a corrupted
backup. All ten responses reported zero reasoning tokens.

Decision: **do not admit Ultra.** Its median was slightly slower than Ling's
0.901s, its 11.491s tail was much worse than Ling's 1.224s P95, and it did not
offer better fidelity. No remaining free OpenRouter route clears both the
product's speed and general-quality bars in this pass.
