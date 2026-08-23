# Usage Statistics Parity

## Canonical Source

- Windows state and header parsing: [src/usage_stats.rs](../../src/usage_stats.rs)
- Windows UI: [src/gui/settings_ui/global/usage_stats.rs](../../src/gui/settings_ui/global/usage_stats.rs)
- Android state and header parsing: [ModelUsageStats.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset/ModelUsageStats.kt)
- Android UI: [UsageStatsDialog.kt](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/ui/UsageStatsDialog.kt)
- Endpoint names, quotas, performance, and enabled state: [catalog/model_catalog.json](../../catalog/model_catalog.json)

## Behavior Contract

- One row represents one API endpoint identity: normalized provider plus exact API
  `full_name`. Text, vision, audio, and search roles that share that identity
  collapse into one row.
- The representative catalog item is the enabled item with the lowest typical
  latency, then the lowest durable ID. It supplies the localized name,
  performance prefix, and static quota label.
- Identical API names from different providers never share usage state.
- Header-derived data is an in-memory snapshot of the latest observed response,
  not historical analytics. Every snapshot records observation time and the UI
  labels fresh, aging, and stale data.
- Quota metrics remain typed and independently labeled. Request/day,
  request/minute, token/minute, token/day, audio-second/hour, and
  audio-second/day buckets are never concatenated into an ambiguous counter.
- A missing live snapshot shows only the catalog quota. It does not add a
  per-endpoint "not observed" marker or fabricate remaining values. The one
  dialog-level empty message names Groq and OpenRouter as the
  providers whose response paths currently feed observed usage.
- Provider-scoped quotas appear once in the provider header, not once per
  endpoint. OpenRouter's free request/day allowance is provider-scoped.
- The list includes every enabled provider represented in the current catalog.
  Provider settings hide providers that have an explicit disabled toggle.
  NVIDIA follows its shared provider toggle on both Windows and Android; it is
  never omitted from Android merely because no rate-limit snapshot exists yet.
  Runtime-local providers are excluded because they have no provider-side API
  usage or quota; their installation and availability belong in model selectors
  and Downloaded Tools instead.
- Rows always show the catalog performance prefix, short localized name, and
  exact API `full_name` on one identity line. The localized name is the primary
  readable label; the inline API ID is quieter monospace metadata. Compact typed
  metrics and freshness replace the static quota when observed. The provider
  icon appears once in its section header. Any clipped identifier or metric
  detail remains available as a hover diagnostic.
- The Windows overview uses two height-balanced provider lanes at the normal
  settings-window width and dense endpoint rows. Its body claims the available
  viewport height instead of shrink-wrapping. Each lane is a borderless table:
  intelligence, latency, localized name, API ID, and quota/live usage have
  content-independent column starts. Provider headers show only the provider
  identity and optional usage link at stable anchors; they do not repeat an
  endpoint count. It falls back to one lane for narrow viewports. Scrolling is
  overflow protection for custom endpoints or small screens, not the primary
  navigation.
- Layout reflows instead of clipping long names or quota data. There is no
  manual spacer used to imitate a column.

## Failure And Recovery

- Missing, malformed, or non-UTF-8 rate-limit headers are ignored
  independently; valid sibling metrics still update.
- HTTP error responses, including 429, are eligible to update a snapshot before
  normal error handling whenever the transport exposes their headers.
- Closing or reopening the dialog does not clear session snapshots. Restarting
  the app clears them and the UI describes them as session observations.
- Providers that do not expose useful rate-limit headers retain their static
  catalog quota and dashboard link.

## Fixtures

- Shared fixture: [parity-fixtures/usage-statistics/contract.json](../../parity-fixtures/usage-statistics/contract.json)
- Windows tests: `usage_stats::tests`
- Android tests: `ModelUsageStatsTest` and `UsageStatsPresentationTest`

## Deviations

- Windows favors a two-lane desktop matrix with compact inline status text.
  Android retains touch-sized Compose sections and wrapping chips because a
  phone cannot preserve the desktop table geometry. Its name and API ID remain
  on one line, but the desktop-only fixed columns do not apply. Identity,
  ordering, labels, freshness thresholds, provider class, and scope remain
  shared.
