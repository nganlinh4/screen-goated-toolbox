# Model Catalog Contract

`model_catalog.json` is the single source of truth for built-in model identity,
endpoint presentation profiles, provider routing, defaults, retry chains,
capabilities, ordinary reasoning policy, ordinary vision request policy, and
display performance metadata.
Windows and Android generated catalogs must consume it; feature code must not
create a second model registry.

## Endpoint Profiles

`model_profiles` is keyed by `<provider>:<exact API full_name>`. Localized name,
daily request quota, search support, default search-tool behavior, intelligence
tier, and ordinary reasoning policy live there once. Modality rows resolve the
profile from their provider and `full_name`.

This guarantees that the same provider endpoint has the same base user-facing
name and policy in text, vision, and audio lists while allowing two providers
to serve an identically named upstream model under different quotas or
policies. A model row may reference a catalog-owned `presentation_variant`
only to distinguish two behavioral rows that share the same endpoint; the
variant appends its localized suffix to the endpoint's base name. Never copy
profile-owned fields back into a model row or infer a built-in capability from
substrings in its ID/name.

## Internal ID Namespace

Built-in IDs use lowercase ASCII kebab-case:

`<provider>-<family>-<version-or-variant>-<capability>`

- Provider is first: `google`, `groq`, `openrouter`, `taalas`,
  `qrserver`, or `local`.
- Capability is last: `text`, `vision`, `audio`, or `search`.
- The API endpoint belongs only in `full_name`; never copy slashes or provider
  aliases into the internal ID.
- Omit mutable lifecycle words such as `preview`, `latest`, `experimental`,
  `stable`, and `deprecated`.
- Use numeric version components separated by hyphens (`3-5`, `1-7b`).
- IDs are immutable and never reused. An intentional namespace rewrite is a
  breaking catalog revision; unknown saved IDs fall back by block type.
- Do not add an in-process migration table. A future persisted-data migration,
  if genuinely required, must be a bounded versioned config upgrade with a
  removal date rather than permanent lookup aliases.

Examples:

- `google-gemini-3-5-flash-lite-vision`
- `groq-qwen-3-6-27b-vision`
- `groq-gpt-oss-120b-text`
- `local-qwen-3-asr-1-7b-audio`

## Localized Display Names

Every built-in localized base name is a short provider prefix plus a neutral
performance specialty. Rendered names must be unique within the provider-prefix
group in each locale, and one API `full_name` must have exactly one localized
base-name triplet across all modalities.

| Provider group | Prefix |
| --- | --- |
| Google, Gemini Live, Google GTX | `GG` |
| Groq | `G` |
| OpenRouter | `O` |
| Taalas | `T` |
| Local runtimes | `L` |
| QRServer | `QR` |

Prefer the shortest useful description, for example `GG Chuẩn`, `G Gọn`, or
`G Gọn`. A raw API/model version is not a user-facing
specialty.

Do not put modality/capability words (`Ảnh`, `Chữ`, OCR, positioning), quota or
lifecycle words (`Giới hạn`, `sắp dừng`, `thử nghiệm`), or implementation terms
such as `suy luận` in a base name. Use `Kỹ`, not `dài dòng`. Keep `Live cũ`,
`Live`, and `Dịch` as neutral specialties instead of error/OCR/machine wording.
The explicit input-transcription behavior is the narrow exception: its
catalog-owned suffix is `(Chép)`, with equivalent Korean and English suffixes.
Apply equivalent restrictions in every locale.

## Quota Labels

Built-in quota text describes daily request count only:

- Vietnamese: `<N> lượt/ngày` or `Không giới hạn`
- Korean: `<N>회/일` or `무제한`
- English: `<N> requests/day` or `Unlimited`

All three locales must carry the same count. `Unlimited` means the provider
does not publish or enforce a daily request-count ceiling for that endpoint; it
does not claim the absence of RPM, TPM, daily token, file-size, concurrency, or
account-tier limits.

The 2026-08-18 audit re-verified every built-in quota label against the active
project's signed-in AI Studio rate-limit page, Groq's official free-plan
rate-limit table, and Groq's live `x-ratelimit-*` response headers. All twenty
labels matched and none were changed. Paid-only endpoints are excluded from the
built-in catalog. Recheck the exact active project/account before changing labels
because provider tiers and project-specific limits can change.

Quota is a distinct axis from latency and accuracy, and it can disqualify an
endpoint from leading a chain on its own: a token-per-minute ceiling low enough to
throttle normal bursts makes an otherwise excellent model a poor first choice. See
the 2026-08-18 decision record for the worked example.

## Performance Metadata

Every enabled built-in has:

- `intelligence_tier`: integer 1–6, rendered as one Material stat icon from
  `stat_minus_3` through `stat_3`.
- `typical_latency_ms`: positive integer, rendered in seconds.
- `performance_source`: benchmark/result identifier or a dated curated source.

Text and vision values come from `tests/catalog-benchmark/`; non-comparable
audio/local utilities use conservative dated curation. Custom/discovered models
show aligned em dashes until measured. Quality measures successful-output
capability; reliability, quota behavior, variance, and lifecycle still affect
the localized specialty and retry priority.

Catalog performance changes require a catalog-ready row in the local latest-run
report described by `tests/catalog-benchmark/README.md`. The selector uses only
the newest complete protocol-compatible run for each model, suite, endpoint,
and reasoning policy. Older runs remain auditable history but never contribute
to current values.

Text latency is the latest run's median end-to-end completion time across all
ten levels. Vision uses the OCR row's successful inputs whose effective longest
edge is at most 1024 px. This representative small-image OCR cohort owns
`typical_latency_ms`; all ten OCR levels still own accuracy and reliability,
while all-case median/P95 remain large-image stress diagnostics. Coordinate
rows are separate Computer/Phone Control evidence and never enter the general
image-to-text latency. A vision row needs at least four successful
representative cases. Reliability counts every attempt in the selected run,
including errors. Diagnose surprising results with TTFO, generation duration,
post-first-output throughput, output length, and recorded image dimensions;
never substitute a different provider's result for a same-family endpoint.

Latency labels round to one decimal second and omit `.0`: `800` → `0.8s`,
`1050` → `1.1s`, `20000` → `20s`.

Expanded model lists sort globally by `typical_latency_ms` regardless of
provider, with durable model ID as the final tie-breaker. The latency column is
fixed-width, end-aligned, and uses tabular numerals. This is presentation order
only; it must never reorder runtime retry chains.

## Reasoning Policy

Every endpoint profile declares `reasoning_policy`; it is generated into both
platforms and must not be reimplemented with model-name heuristics.

- Ordinary Gemini 2.5/Robotics calls use a zero thinking budget where supported.
- Ordinary Gemini 3/Gemma calls use the provider's minimum thinking level.
- Ordinary OpenAI-compatible reasoning models use `none` where supported and
  otherwise the lowest supported level (`low`).
- OpenRouter encodes that policy as nested
  `reasoning: { effort: "<level>" }`; Groq uses `reasoning_effort`.
- Gemini Live uses its exact endpoint profile.
- Provider-managed compound/search behavior is explicit.
- Help Assistant and Computer/Phone Control are correctness-sensitive
  exceptions and use bounded `LOW` thinking. Thought output is exposed only to
  the control runtime, never to ordinary model calls.

## Vision Request Policy

`vision_request_profiles` is keyed by `<provider>:<exact API full_name>` and
must cover every enabled ordinary Google, Groq, and OpenRouter vision
endpoint.
It owns input-part order, media-resolution policy, fast sampling profile,
optional output-token ceiling, and structured-output wire policy. Windows and
Android generate these facts from the catalog; do not infer them from a
family-name substring.

The current policies come from production-path transport probes:

- Google vision endpoints use image-first ordering. Groq Qwen uses
  text-first ordering.
- Media resolution remains provider-default. Lower resolution reduced Gemini
  input-token accounting but did not produce a durable end-to-end latency win.
- Qwen 3.6 uses the Groq-accepted subset of its documented non-thinking
  sampling profile (`temperature: 0.7`, `top_p: 0.8`,
  `presence_penalty: 1.5`) together with the separate catalog-owned
  `reasoning_effort: none`. Do not send `top_k` or `min_p`: live endpoint
  probes return HTTP 400 for both fields.
- Every ordinary vision endpoint reserves 512 output tokens. A production-path
  OCR probe used 220 completion tokens, while the ten benchmark responses were at
  most 390 characters, so the ceiling is generous for extraction while bounding
  what a single call can cost. On Groq it also reduced TPD admission from 4,483
  to 2,937 tokens for the same small-image request without truncation, because a
  reserve is charged whether it is used or not. The value is uniform so that
  image behaviour does not change with the endpoint a chain happens to reach;
  roughly 250-380 words depending on language, which bounds prose replies too.
- `structured_output` is a wire policy, not a default. `prompt-only` means a
  model is asked for structure in the prompt without attaching a schema;
  `json-object` and `strict-json-schema` select documented constrained modes.
  Plain OCR normally requests plain text. The one exception is a `json-object`
  endpoint serving a non-streaming plain-text caller: those requests carry a
  `{"text": ...}` envelope and are unwrapped on the way out, because Qwen 3.6
  otherwise appends a re-tokenized repetition of the text it just emitted. The
  envelope is a wire detail; callers still receive plain text. A structural caller must provide a schema, and
  the provider adapter may attach it only when the exact profile allows it.

OCR catalog timing measures full-answer completion through the real
non-streaming preset path. A diagnostic streaming probe may record time to
first output, but must not substitute that value for product completion time.
Increment the benchmark protocol before registering runs after any request
profile changes.

## Search Capability

`supports_search` means the exact provider-qualified endpoint can accept its
provider's search tool. It preserves capability when an explicitly
search-enabled retry chain falls back. It does not authorize ordinary
translation, refinement, OCR, or transcription requests to invoke search, and
it must never directly control a search marker.

`search_tool_enabled_by_default` is the separate behavioral fact used by the
model-list marker. It is true only when selecting that endpoint in the normal
model path actually enables or invokes provider search. General Google models
therefore keep `supports_search: true` but set this field to false: their
quota-bearing grounding tool is reserved for a dedicated explicit feature
path. Groq Compound sets both fields to true because its normal production
request enables provider-managed web tools.

Verify each true value against the provider's current model-capability page.
Ordinary requests omit search tools so grounding billing/quota cannot turn a
normal model call into a quota error. Search-specialized models and explicitly
requested Computer/Phone Control turns may attach the tool through their
dedicated path.

When adding or changing an endpoint, inspect the actual production payload and
decide both fields independently. The validator rejects a default-enabled
search tool without provider support, and the shared presentation fixture locks
the exact built-in rows that may display the marker.

## Priority Policy

Default retry chains optimize interactive completion: latency is weighted more
heavily than small intelligence differences once a model clears the task's
accuracy floor. Availability and consistency are hard gates, followed by
latency, accuracy, provider diversity, quota, and endpoint lifecycle. A
translation-only service, search-specialized model, soon-retired endpoint, or
model with nonrepresentative sparse evidence must not lead a general chain.

Record a reviewed dated decision from the newest complete protocol-compatible
run before changing intelligence tier, latency, or default priority. Focused
quota/error recoveries may be merged into that one logical run before it is
registered; recovery fragments must never be registered as separate results.
Automatic text accuracy remains only an aid for rubric-based human judgment.

## Default Adoption

`constants`, `preset_defaults`, `provider_defaults`, and `priority_chains` are
one recommendation set. Rust and Android defaults must be generated from these
sections; do not repeat provider booleans or model IDs in platform config code.

Before replacing the Windows executable, the updater snapshots the old
recommendations. After restart, it offers the one-time prompt only when preset
model slots, priority chains, or recommended providers changed. Applying it
updates changed built-in model slots, restores both priority chains, and
enables every currently recommended provider. Provider activation is additive:
never disable another provider the user already enabled. Skipping consumes the
marker and preserves all current settings.

Android has no equivalent self-update staging hook. Clean or explicitly reset
runtime settings consume the generated recommendations; persisted Android
choices remain unchanged.
