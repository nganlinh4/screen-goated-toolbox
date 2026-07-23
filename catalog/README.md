# Model Catalog Contract

`model_catalog.json` is the single source of truth for built-in model identity,
localized labels, provider routing, defaults, retry chains, and display
performance metadata. Windows and Android generated catalogs must consume it;
feature code must not create a second model registry.

## Internal ID Namespace

Built-in IDs use lowercase ASCII kebab-case:

`<provider>-<family>-<version-or-variant>-<capability>`

- Provider is first: `google`, `groq`, `cerebras`, `taalas`, `qrserver`, or
  `local`.
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
- `cerebras-gpt-oss-120b-text`
- `local-qwen-3-asr-1-7b-audio`

## Localized Display Names

Every built-in localized name is a short provider prefix plus a capability or
performance specialty. Names must be unique within the provider-prefix group
in each locale.

| Provider group | Prefix |
| --- | --- |
| Google, Gemini Live, Google GTX | `GG` |
| Groq | `G` |
| Cerebras | `C` |
| Taalas | `T` |
| Local runtimes | `L` |
| QRServer | `QR` |

Prefer the shortest useful description, for example `GG Chính xác, chậm`,
`C Tốt`, or `G Không ổn định`. A raw API/model version is not a user-facing
specialty.

## Performance Metadata

Every enabled built-in has:

- `quality_tier`: integer 1–5, rendered as that many brain icons.
- `typical_latency_ms`: positive integer, rendered in seconds.
- `performance_source`: benchmark/result identifier or a dated curated source.

Text and vision values come from `tests/catalog-benchmark/`; non-comparable
audio/local utilities use conservative dated curation. Custom/discovered models
show aligned em dashes until measured. Quality measures successful-output
capability; reliability, quota behavior, variance, and lifecycle still affect
the localized specialty and retry priority.

Latency labels round to one decimal second and omit `.0`: `800` → `0.8s`,
`1050` → `1.1s`, `20000` → `20s`.

## Priority Policy

Default retry chains are based on accuracy first, then availability,
consistency, latency, provider diversity, and endpoint lifecycle. A
translation-only service, search-specialized model, soon-retired endpoint, or
model with nonrepresentative sparse evidence must not lead a general chain.

Run the ten-level benchmark and record a dated result before changing quality,
latency, or default priority.
