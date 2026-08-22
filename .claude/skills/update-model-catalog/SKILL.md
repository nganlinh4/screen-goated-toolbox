---
name: benchmark-model-catalog
description: Run one unified model-discovery and production-path benchmark day, human-review model quality, and apply verified catalog decisions for Screen Goated Toolbox.
---

# Benchmark Model Catalog

Discovery is the first phase of a benchmark day, never a separate workflow. The
same session verifies current provider state, benchmarks every runnable general
model, performs human quality review, audits production request parity, and only
then changes the shared catalog when authorized.

## Start

1. From the repository root, read `.claude/commands/manage-model-catalog.md`, `catalog/README.md`, `tests/catalog-benchmark/README.md`, and `tests/catalog-benchmark/history-policy.json` completely.
2. Inspect `git status --short`; preserve unrelated work.
3. Treat discovery and benchmarking as read-only evidence unless catalog mutation is explicitly authorized.
4. For cross-platform catalog changes, read `.claude/skills/enforce-mobile-parity/SKILL.md`, `.claude/parity/model-catalog.md`, and the affected feature parity spec before editing.

## Unified discovery phase

- Use only current first-party documentation, provider list-model APIs, and
  authenticated consoles. Record the observation date and exact endpoint ID.
- Google: inspect the signed-in AI Studio project limits and official
  [rate-limit](https://ai.google.dev/gemini-api/docs/rate-limits), model,
  lifecycle, API-version, and capability pages. Rate limits are project- and
  model-specific.
- Groq: inspect official
  [supported-model](https://console.groq.com/docs/models) and
  [rate-limit](https://console.groq.com/docs/rate-limits) pages plus account-visible
  list-models and structural response headers. Limits are enforced at organization
  and project/model scopes; adding keys in one project does not invent quota.
- OpenRouter: define free only from parseable zero prompt and completion pricing;
  confirm the current [shared free allowance](https://openrouter.ai/docs/faq).
  Its free request allowance is account-wide, so keep one provider-wide shard.
- NVIDIA: verify the current
  [hosted LLM API](https://docs.api.nvidia.com/nim/reference/llm-apis), then refresh
  the signed availability feed before selection. Benchmark every
  currently offered curated or discovered general endpoint with the feed's verified
  reasoning control. Do not turn a narrow preset sample into catalog-wide quality.
- Treat missing access, quota exhaustion, lifecycle removal, and malformed request
  policy as different findings. Never expose credentials or authenticated URLs.

Generate the current OpenRouter inventory with:

```powershell
py -3 .claude/skills/update-model-catalog/scripts/openrouter_free_models.py
```

Provider rankings shortlist candidates; only the production-path benchmark owns
SGT performance evidence.

## Verify request parity before judging output

1. Compare discovery with `catalog/model_catalog.json` by provider plus exact `full_name`.
2. Check endpoint URL, modality, input part order, MIME handling, streaming mode,
   output ceiling, schema mode, sampling, search tools, and the provider's lowest
   supported reasoning control against current first-party docs.
3. Trace the benchmark call through the same Rust `translate_text_streaming` or
   `translate_image_streaming` entry point as the application. The benchmark must
   inherit production retries, feed/catalog reasoning controls, vision profiles,
   parsing, and repetition protection; never reproduce the payload separately.
4. If current best practice changes, repair the shared production adapter/profile
   first so operation and benchmark change together, then bump the protocol.
5. Exclude paid-only, dedicated non-LLM, and search-by-default services from the
   general suite; test dedicated capabilities only through their own contract.

Do not infer sibling capabilities from a family name. Capability and default behavior are separate catalog facts.

## Run the benchmark day

1. Use the ignored Rust benchmark in `tests/catalog-benchmark`; do not replace it with ad hoc HTTP timing.
2. Ensure child processes receive the intended `.env` values explicitly. A saved app key can otherwise mask an edited `.env`. Compare only non-secret fingerprints when diagnosing key selection.
3. Run all ten round-major levels for every runnable general model, including
   NVIDIA feed candidates. Use focused filters only for screening or recovery.
4. Preserve request errors, malformed responses, overloads, retries, and quota failures. They are reliability evidence, not latency samples.
5. Use resume inputs to skip successful cells. Merge recovery reports left-to-right into one logical run; never register a recovery fragment independently.
6. Use only the latest complete protocol-compatible run. Do not average older runs.
7. Measure latency only from request start until the complete result returns.
   Never rank with time-to-first-token or post-first-token throughput.
8. Automatic similarity and constraint metrics only triage review. Complete every
   text verdict, 1–5 rating, and rubric check in `reviews.json`; an unreviewed row
   is not decision-ready. Use OCR scores as aids and coordinate strict success as
   control evidence.
9. Keep ordinary reasoning disabled or at the endpoint's documented minimum.
10. Maximize legitimate quota: rotate independent credential arrays, shard
    model-scoped providers, keep account-wide providers serialized, retain
    structural retry/quota failures, and use resume/merge instead of wasting
    successful cells.

For shortest wall time, dispatch `.github/workflows/catalog-benchmark-day.yml`.
It compiles one Windows test binary, distributes quota-safe shards across hosted
runners, merges fragments, and uploads one human-review queue. Local runs remain
the fallback for credentials or runtimes that are intentionally not hosted.

Do not update vision latency without the catalog policy's minimum representative successful small-image cohort.

## Apply Decisions

Edit `catalog/model_catalog.json` as the sole owner. Update all affected sections together:

- endpoints and endpoint profiles;
- localized names, quotas, intelligence, reasoning and search behavior;
- modality rows and vision request profiles;
- constants, provider defaults, presets, and retry chains;
- lifecycle data and aliases where permitted.

Base retry order on reviewed quality and reliability first, then full-result
latency, provider diversity, quota, and lifecycle. Keep authority-bearing
Computer/Phone Control on its separate validated chain.

Update parity specs, fixtures, focused assertions, and post-update recommendation expectations with the same decision. Never create a second platform model registry.

## Validate

Run at minimum:

```powershell
py -3 scripts/generate_android_preset_model_catalog.py --manifest-source catalog/model_catalog.json --validate-only
cargo fmt
cargo test
cargo clippy --all-targets -- -D warnings
git diff --check
```

Run Android checks from `mobile/README.md` when generated Android behavior or a shared parity contract changes. Report discovery evidence, accepted/rejected candidates, benchmark coverage and quota gaps, catalog/default/priority changes, and validation results.
