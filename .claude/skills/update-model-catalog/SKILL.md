---
name: update-model-catalog
description: Discover, verify, benchmark, add, remove, rename, or reprioritize built-in AI models in Screen Goated Toolbox. Use for provider catalog refreshes, AI Studio quota audits, Groq or Cerebras model/rate-limit checks, OpenRouter free-model discovery, live production-path model comparisons, catalog latency/intelligence updates, preset/default changes, or retry-chain maintenance.
---

# Update Model Catalog

Maintain the shared catalog from current provider evidence through verified product behavior.

## Start

1. From the repository root, read `.claude/commands/manage-model-catalog.md`, `catalog/README.md`, `tests/catalog-benchmark/README.md`, and `tests/catalog-benchmark/history-policy.json` completely.
2. Inspect `git status --short`; preserve unrelated work.
3. Decide whether the request is discovery-only, benchmark-only, or authorizes catalog mutation. Discovery and benchmarking do not imply edits.
4. For cross-platform catalog changes, read `.claude/skills/enforce-mobile-parity/SKILL.md`, `.claude/parity/model-catalog.md`, and the affected feature parity spec before editing.

## Discover Current Provider State

Read [provider-discovery.md](references/provider-discovery.md) for the exact provider routes and evidence rules.

- Prefer official provider documentation and authenticated provider consoles over search results.
- Use Chrome control for signed-in AI Studio state. Use the in-app browser or normal web access for public official documentation.
- Record exact API model ID, lifecycle, modalities, free availability, request quotas, tool support, reasoning controls, and structured-output support.
- Treat console quota values as project/account-specific. Never generalize them to another key or billing tier.
- Never expose keys, project IDs, full provider errors containing secrets, or authenticated-page URLs with sensitive query data.

For OpenRouter discovery, run:

```powershell
py -3 .claude/skills/update-model-catalog/scripts/openrouter_free_models.py
```

This produces a current free-model inventory from the official API. Use OpenRouter's official model UI for its live latency/throughput ordering, then benchmark shortlisted endpoints locally; rankings from the site are candidate-selection evidence, not product performance metadata.

## Verify Candidates Before Cataloging

1. Compare discovery results with `catalog/model_catalog.json` by provider plus exact `full_name`.
2. Confirm the endpoint through the provider's list-models API when available.
3. Verify the real request contract: modality, endpoint URL, part ordering, MIME handling, reasoning disable/minimum, structured output, search-tool behavior, and output ceiling.
4. Exclude paid-only providers or endpoints. Do not retain placeholders for providers that violate the product's no-billing requirement.
5. Add a provisional catalog row only when needed to exercise the production dispatcher. Remove rejected candidates completely after evaluation.

Do not infer sibling capabilities from a family name. Capability and default behavior are separate catalog facts.

## Benchmark Through the Product Path

1. Use the ignored Rust benchmark in `tests/catalog-benchmark`; do not replace it with ad hoc HTTP timing.
2. Ensure child processes receive the intended `.env` values explicitly. A saved app key can otherwise mask an edited `.env`. Compare only non-secret fingerprints when diagnosing key selection.
3. Run all ten round-major levels for every applicable suite. Use focused model filters only for candidate screening or recovery.
4. Preserve request errors, malformed responses, overloads, retries, and quota failures. They are reliability evidence, not latency samples.
5. Use resume inputs to skip successful cells. Merge recovery reports left-to-right into one logical run; never register a recovery fragment independently.
6. Use only the latest complete protocol-compatible run. Do not average older runs.
7. Judge translation against its rubric manually. Use OCR automatic scores as aids. Treat coordinate results as control evidence, never general OCR latency.
8. Diagnose surprising latency using preparation time, provider time, first output, completion time, output length, image dimensions, retry wait, P95, and CV.

Do not update vision latency without the catalog policy's minimum representative successful small-image cohort.

## Apply Decisions

Edit `catalog/model_catalog.json` as the sole owner. Update all affected sections together:

- endpoints and endpoint profiles;
- localized names, quotas, intelligence, reasoning and search behavior;
- modality rows and vision request profiles;
- constants, provider defaults, presets, and retry chains;
- lifecycle data and aliases where permitted.

Base retry order on availability and consistency first, then latency-weighted quality, provider diversity, quota, and lifecycle. Keep authority-bearing Computer/Phone Control on its separate validated chain.

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
