# Catalog decisions — 2026-09-16, protocol 15

This checkpoint includes 1,000 baseline attempts across 100 model/suite groups: ten cases each for 42 text and 29 vision endpoints (OCR and coordinates separately). It combines the compatible 580-cell selected-catalog run with 420 new cells for all 20 additional eligible endpoints. Previously rejected candidates were retested. Dedicated, paid-only and incompatible contracts remain outside the general suite.

601 baseline results returned. All 277 returned text responses received direct Codex reviews with ratings and rubric checks; these are not human sign-off. Another 222 structured-translation diagnostics yielded 95 reviewed results and are excluded from ordinary text history. All cells were attempted, but quota/access failures leave incomplete quality evidence for several candidates.

The user explicitly authorized catalog updates. Raw attempts, output references, reviews, prior failures, hashes and the full report are in `target/catalog-benchmark/2026-09-16-all/`; the original sealed evidence remains in `target/catalog-benchmark/2026-09-16-complete/`. The expanded registered logical run is `import-20260916T1408202906045000000-5355ba499193`. Recovery fragments were not independently registered.

## Applied decisions

- Admit Gemini 3.8 Live text with tier 5 and a 3446 ms measured full-result median; add it as a late text fallback.
- Refresh 33 catalog text/OCR latency values from protocol 15. Preserve older labels where current samples are insufficient, rather than relabeling them as current evidence.
- Keep Qwen 3.8 as both generic heads: 322 ms text and 856 ms representative OCR. Coordinate evidence does not change the separate control chain.
- Promote Gemini 3.1 Flash Lite to the first Google text fallback (10/10, quality 5/5, 1262 ms); retain a late OpenRouter fallback for provider diversity.
- Make Gemini 3.5 Flash Lite the immediate image fallback, followed by 3.1 Flash Lite and 3 Flash. Move less reliable Robotics and dots later; remove 3.7 Flash from the recommended image chain.
- Preserve existing manual selectability for sparse-evidence endpoints; transient quota/access failures do not justify retiring an established endpoint. Gemini 2.5 Flash Lite retains its prior disabled state.
- Remove all unqualified provisional candidate rows; record each current blocker separately from prior rejection history. No provisional latency or intelligence placeholders remain.
- Update Windows/Android generated Live endpoint completion profiles and parity fixtures; ordinary endpoints retain their existing request and completion semantics.
- Update benchmark-day skill to reconcile discovery, prior rejections/reversals, disabled entries, current attempts and authorized catalog decisions before claiming closure.

The new Live text profile uses no thinking configuration, matching its documented default. The extended-thinking endpoint was tested at its supported low setting and requires explicit interaction-idle completion. These opt-in profiles do not alter existing endpoints, benchmark fixtures or scores, so compatible same-day protocol-15 cells were reused.

## All additional candidates

Provider identity is part of every decision. OpenRouter evidence does not transfer to a direct NVIDIA route or vice versa.

| Provider / exact endpoint | Prior finding | Current decision |
| --- | --- | --- |
| openrouter: `inclusionai/ling-3.0-flash-vl:free` | No exact-endpoint rejection found in the reviewed decision records. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| openrouter: `nex-agi/nex-n2.5-mini:free` | No exact-endpoint rejection found in the reviewed decision records. | Blocked: vision contract; text incomplete. Vision HTTP 400 persists with a larger image and non-streaming diagnostic. Text has only three successes, long stalls and quota failures. |
| openrouter: `nex-agi/nex-n2.5-pro:free` | No exact-endpoint rejection found in the reviewed decision records. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| openrouter: `inclusionai/ling-3.0-flash-sante:free` | No exact-endpoint rejection found in the reviewed decision records. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| openrouter: `inclusionai/ling-3.0-flash-fin:free` | No exact-endpoint rejection found in the reviewed decision records. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| openrouter: `liquid/lfm-2.5-2.6b:free` | No exact-endpoint rejection found in the reviewed decision records. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| openrouter: `nvidia/nemotron-3.5-lightning:free` | August reasoning-leak rejection later withdrawn after correcting reasoning settings; NVIDIA evidence does not substitute for this route. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| openrouter: `thinkingmachines/inkling-small:free` | No exact-endpoint rejection found in the reviewed decision records. | Blocked: route access. HTTP 403 restricts this free route to approved agentic harnesses; no usable outputs. |
| openrouter: `poolside/laguna-s-2.1:free` | Rejected in July for fidelity/latency; August screen found ignored response_format. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| openrouter: `thinkingmachines/inkling:free` | No exact-endpoint rejection found in the reviewed decision records. | Blocked: route access. HTTP 403 restricts this free route to approved agentic harnesses; no usable outputs. |
| openrouter: `poolside/laguna-xs-2.1:free` | Rejected for text quality in the August 10 record. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| openrouter: `cohere/north-mini-code:free` | No exact-endpoint rejection found in the reviewed decision records. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| openrouter: `z-ai/glm-5.2:free` | August 18 screen: HTTP 429 in three of four probes. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| openrouter: `nvidia/nemotron-3-ultra-550b-a55b:free` | July 24 screen: semantic errors and poor tail latency; not admitted. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| openrouter: `nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free` | No exact-endpoint rejection found in the reviewed decision records. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| openrouter: `google/gemma-4-31b-it:free` | Rejected for availability or strict vision behavior in the August 10 record. | Deferred: quota/access. All ten cases attempted per modality. Shared daily quota and upstream failures left insufficient hard-case evidence; no semantic rejection inferred. |
| google: `gemini-2.5-flash` | No exact-endpoint rejection found in the reviewed decision records. | Blocked: project access. Predominant project-access 404s; only one text success. Flash Lite remains disabled; Flash is not added. No global retirement inferred. |
| google: `gemini-2.5-flash-lite` | Disabled after August project-access 404s. Current project access was retested; old lifecycle dates are not assumed current. | Blocked: project access. Predominant project-access 404s; only one text success. Flash Lite remains disabled; Flash is not added. No global retirement inferred. |
| gemini-live: `gemini-3.8-live` | No exact-endpoint rejection found in the reviewed decision records. | Admit text only. 10/10 text results, reviewed mean 4.7/5, median 3446 ms. OCR similarity 0.793 and 3/10 strict coordinate passes do not justify ordinary vision admission. |
| gemini-live: `gemini-3.8-live-extended-thinking` | No exact-endpoint rejection found in the reviewed decision records. | Reject current output quality. Text mean 3.5/5 includes three completed error apologies. OCR similarity 0.773. Explicit interaction-idle completion was used, so these are not prematurely cut responses. |

Shared free-request exhaustion is an account quota finding, not a quality rejection. Existing sparse-evidence catalog rows retain their prior manual availability and older measured labels; those labels are not claimed as protocol-15 evidence. The next recovery should request only missing/failed cells after access or quota changes, with the same production profiles and protocol. Do not replace preserved failures with zero latency or average older runs.

## Ranking rationale

Qwen 3.8 retains both generic heads: text 10/10, reviewed 4.7/5, median 322 ms; OCR 9/10, similarity 0.974, representative median 856 ms. Its coordinate result was 0/10 strict, so this decision does not promote it to Computer/Phone Control.

Gemini 3.1 Flash Lite text returned 10/10, reviewed 5/5, median 1262 ms and a verified 500-request daily allowance. It precedes the 3.5 Lite text fallback (4.6/5, 1264 ms). For OCR, 3.5 Lite and 3.1 Lite each returned 10/10, with representative medians 1959 and 2360 ms; they precede less reliable Robotics (7/10) and dots (8/10). Gemma 26B remains behind higher-token-budget choices despite strong output quality because the active free project has a 16K TPM ceiling.

Latency labels use complete-result medians. Ordinary vision uses at least four successful representative OCR inputs at most 1024 px on the longest effective edge; coordinate latency and large-image stress timing do not own these labels. Intelligence tiers for established models remain unchanged: this small translation/rewrite suite alone does not redefine broad model capability.

## Sources and validation

Current primary sources: [Google pricing](https://ai.google.dev/gemini-api/docs/pricing), [Live model contract](https://ai.google.dev/gemini-api/docs/models/gemini-3.8-live), [extended-thinking contract](https://ai.google.dev/gemini-api/docs/models/gemini-3.8-live-extended-thinking), [Live wire protocol](https://ai.google.dev/api/live), [OpenRouter inventory](https://openrouter.ai/api/v1/models), and [reasoning controls](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens). The signed-in free-project AI Studio page was refreshed on September 16: both new Live endpoints show unlimited daily requests and 65K TPM. Unlimited is a daily request-count label, not unlimited tokens or concurrency.

Validation commands and results are recorded in the local report's `validation.json`. Shared recommendation and Live completion fixtures cover Windows and Android Full/Play consumers. No release or production component promotion was performed.
