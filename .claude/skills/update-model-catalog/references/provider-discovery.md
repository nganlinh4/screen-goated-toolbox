# Provider Discovery

Use only current first-party sources. Provider pages and API schemas change; inspect them live instead of copying a permanent model list here.

## Google Gemini

1. Use Chrome control to open the user's signed-in AI Studio rate-limit page.
2. Select the intended project and time window, enable/show all models, and inspect every displayed endpoint. Record exact model ID and project-specific RPM, TPM, and RPD where shown.
3. Cross-check the official Gemini model list, deprecation table, API-version matrix, and relevant capability pages.
4. Query the Gemini list-models API with the intended key when endpoint availability is uncertain.
5. Distinguish model quota exhaustion from project/key selection, unavailable preview access, and billing-only access.

Use `.claude/skills/gemini-api-dev/SKILL.md` for Gemini wire-contract changes and `.claude/skills/gemini-live-api-dev/SKILL.md` for Live endpoints.

## Groq

1. Read Groq's official supported-model and rate-limit documentation.
2. Query Groq's OpenAI-compatible list-models endpoint with the configured key when verifying account-visible IDs.
3. Confirm exact modality, lifecycle, structured-output/tool support, and reasoning controls from first-party material.
4. Treat rate-limit response headers gathered by the product as account-specific runtime evidence.

## OpenRouter

1. Run `scripts/openrouter_free_models.py` for the official API inventory.
2. Define free as zero prompt and completion price. Treat missing or unparsable pricing as unknown, not free.
3. Use OpenRouter's official model browser to inspect its current latency-low-to-high and throughput-high-to-low views. Capture the model IDs and ordering, not screenshots containing account data unless review needs them.
4. Exclude endpoints requiring billing, unavailable to the active key, lacking the needed modality, or already represented by the same provider-qualified endpoint.
5. Shortlist several diverse candidates, then use production-path Rust tests. OpenRouter aggregate rankings do not substitute for SGT latency, reliability, OCR, coordinate, or translation evidence.

The OpenRouter API key is optional for public inventory but required for live candidate tests. Inject `.env` values into the benchmark child explicitly; do not print them.

## Evidence Discipline

- Keep source URL and observation date in working notes, but store mutable facts in the catalog rather than duplicating model lists in skill prose.
- Prefer exact provider-qualified endpoint identity over marketing names.
- A search-capable endpoint does not automatically enable search in normal product calls.
- Separate published quotas, authenticated-console quotas, response-header observations, and benchmark failures.
- If official sources disagree, retain the safer existing behavior and report the conflict before mutation.
