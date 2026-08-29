# Preset System Parity

## Canonical Source

- Windows editor/catalog: [preset settings](../../src/gui/settings_ui/preset.rs), [preset model](../../src/config/preset/preset.rs)
- Windows execution: [chain](../../src/overlay/process/chain), [text input](../../src/overlay/text_input), [result](../../src/overlay/result), [favorite bubble](../../src/overlay/favorite_bubble)
- Shared model catalog: [catalog/model_catalog.json](../../catalog/model_catalog.json)
- Android preset model/runtime: [shared preset](../../mobile/shared/src/commonMain/kotlin/dev/screengoated/toolbox/mobile/shared/preset), [Android preset](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset), [overlay host](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset)

## Data and Editor Contract

- Windows built-ins are canonical seed data. Android persists user overrides by preset ID; restore removes the override.
- The OCR-first `preset_extract_retranslate` and
  `preset_extract_retrans_retrans` built-ins are retired on both platforms.
  Their canonical replacements are `preset_translate` and
  `preset_translate_retranslate`, respectively. The direct replacement owns
  the unmodified backtick default on Windows. When Windows loads older saved
  catalogs, it removes every retired row, transfers its favorite state and
  unique hotkeys to the matching replacement, and redirects an active retired
  selection. Android does not copy Windows-only hotkey metadata into its
  runtime catalog.
- The catalog owns the generic image default, accuracy-sensitive image default,
  preset-specific model defaults, provider defaults, and both retry chains.
  Gemini 3.5 Flash Lite is the broad image default because it combines
  near-fastest small-image OCR with robust coordinate behavior. Built-in image
  translation and accuracy-labeled OCR pipelines, structured tables, fact
  checking, comprehensive extraction, and ask-image all follow that broad
  stable default. Catalog availability or priority-chain membership does not
  make a model a built-in preset default. Authority-bearing Computer/Phone
  Control keeps its separate catalog-owned model chain.
- The fast text-arena seed uses Groq GPT-OSS 20B. The general image retry
  chain keeps Gemini 3.5 Flash Lite first, then prioritizes the fast reliable
  Groq and OpenRouter OCR endpoints before higher-variance fallbacks. The text
  chain keeps Gemini 3.5 Flash Lite first for answer quality, with Groq
  GPT-OSS 20B as its speed-specialized fallback before the remaining hosted
  endpoints. Exact order remains owned only by the shared catalog and
  fixture.
- The Windows one-time post-update recommendation prompt compares the staged
  preset models, priority chains, and recommended-provider defaults. Applying
  updates changed built-in model slots, restores both priority chains, and
  enables every currently recommended provider without disabling any other
  provider. Skipping only consumes the marker. Android has no equivalent
  self-update staging hook: generated recommendations apply to clean/reset
  runtime settings, while persisted Android choices remain unchanged until the
  user resets them.
- Favorite state, ordering, edits, and custom presets are repository-backed, never Compose-only state.
- Android supports preset creation, duplication, deletion, and the current node-graph editor actions. Capability UI must still reflect real runtime support for every block/provider.
- New presets and newly inserted graph nodes resolve their modality default
  through the shared catalog constants; platform editors must not carry a
  literal fallback model ID.
- Unknown/corrupt override fields fall back safely to canonical built-ins.
- Hotkeys and controller/master invocation remain Windows-only until Android has a real equivalent.

## Launch and Overlay Contract

- Presets execute from the floating bubble service. Zero favorites shows a localized empty state.
- The panel preserves Windows keep-open, size, multi-column, overlap, animation, drag/reposition, and refresh semantics through a thin Android bridge.
- Input uses the Windows text-input DOM/CSS/message contract, including submit, cancel, history, close, and working microphone input.
- Permission-gated image/audio paths fail before capture, explain the required Android permission, and preserve retry state.
- Image presets support continuous relaunch. Non-image continuous mode remains a documented gap.
- Result windows are session-owned, precreated in loading state, multi-window, and support markdown streaming or raw HTML according to block render mode.
- Raw HTML is a document lifecycle, not a Markdown DOM update. A complete raw
  document (including one harmless outer `html`/`htm` code fence) is normalized,
  loaded once, and remains authoritative: the host must never replace its body
  through the generic Markdown `innerHTML` updater after load. Authored CSS,
  animations, scripts, and interaction state remain owned by that document;
  host integration is limited to overlay interaction, navigation, diagnostics,
  visibility, and lifecycle bridges.
- Interactive raw-HTML presets choose controls appropriate to the generated experience: keyboard, pointer, touch, or a combination. The built-in Make a Game prompt must not impose a mouse-only design; it requires visible control instructions and keyboard activation after the player clicks or taps the result.
- The result-control badge identifies the endpoint that actually produced the current result after retry/fallback, formatted as provider display name plus the complete API model name; non-model results have no badge.
- Android result and mini-app WebViews follow [the shared Android overlay rendering contract](../../parity-fixtures/android-webview-overlays/rendering-contract.json): each overlay window owns hardware acceleration and its WebView composes directly without a persistent offscreen hardware layer.
- Reuse Windows markdown fitting/theme/font/table and button-canvas contracts. Preserve text selection, one-finger window drag, two-finger bidirectional content scroll, navigation recovery, and result geometry ownership.
- Edit/refine, undo/redo, share/download, and speaker actions are real Android actions. Refine starts with the endpoint that actually produced the displayed text when it is text-capable, then uses the current adaptive Text-to-Text retry chain and circuit policy. Its result badge follows the endpoint that completes the refine. Do not list implemented actions as placeholders.
- Android still omits the desktop markdown/plain toggle and broom mouse-button variants.

## Provider Contract

- Android preset capability checks use the same runtime-dispatch capability
  table as the text, vision, and audio clients. A provider that the client can
  execute—including NVIDIA and Gemini Live—must not be rejected by a stale
  editor or repository allowlist.
- Resolve every internal model ID through generated data from `catalog/model_catalog.json`; call the resolved provider and `full_name`.
- Preserve Windows render-mode, streaming, catalog-owned reasoning/search
  capability, provider-availability, retry, and fallback semantics. Built-in
  capabilities and ordinary reasoning policy are exact endpoint-profile data,
  never model-name heuristics.
- Every ordinary Text-to-Text, refinement, and Image-to-Text result removes only
  leading CR/LF characters before its first visible output. This normalization
  happens before streaming paint and in the returned final result on both
  platforms; leading spaces, tabs, and later line breaks remain untouched.
  Transport replacement signals restart the same initial-output normalization.
- Search capability preserves explicit-search retry compatibility. It does not
  draw a model-list marker: the marker comes only from the separate
  catalog-owned `search_tool_enabled_by_default` behavior flag. Ordinary
  text/audio generation never invokes provider search tools implicitly;
  quota-bearing grounding is enabled only by an explicit search feature path.
- Gemini Live setup uses the catalog-owned output ceiling for each endpoint: 8,192 for Live 2.5 and 65,536 for Live 3.1, on both Windows and Android.
- `Viết liên tục` uses the older Live 2.5 input-transcription row
  (`google-gemini-2-5-live-transcribe-audio`), rendered as `GG Live cũ (Chép)`
  in Vietnamese. The custom-prompt row sharing that endpoint remains
  `GG Live cũ`.
- Every native Live setup envelope is built through the platform's typed setup builder; endpoint policy is applied by construction and feature adapters supply only capability deltas.
- Live server events are decoded structurally. Setup completion must be a top-level field, all audio parts in a frame are retained, and finite responses complete on either `turnComplete` or `generationComplete`.
- Blank, legacy, or unknown Gemini TTS model values normalize to the catalog-owned TTS default on both platforms; listed models remain unchanged.
- Provider/auth failures and retryable model failures remain distinct. Retrying an open result updates its loading status.
- Every enabled provider has a complete encrypted credential path from settings
  state to execution. In particular, NVIDIA availability and requests use the
  persisted NVIDIA key on both platforms; an empty key makes that provider
  unavailable without altering the authored chain.
- A non-streaming plain-text vision caller never requests JSON mode or a schema.
  Endpoint profiles that may restate the requested output use the same
  endpoint-scoped repetition guard on both platforms; unrelated endpoints and
  structured callers are unchanged.
- Ordinary LLM vision request shape comes from
  `catalog/model_catalog.json#vision_request_profiles` on both platforms.
  Google vision endpoints send image before text; Groq Qwen sends text before
  image. Media resolution remains provider-default:
  small-image probes showed no durable completion-latency win from forcing a
  lower setting. Plain OCR is non-streaming because the product consumes the
  complete transcription and the tested endpoints generally buffer their first
  visible output until near completion.
- A caller-supplied vision schema is sent only when the endpoint profile declares
  strict structured-output support. Gemini uses `responseJsonSchema`; endpoints
  without that capability keep their catalog-owned prompt-only or JSON-object
  transport. Plain OCR supplies no schema.
- Vision payloads preserve their real MIME type. Groq images use a prompt-aware
  encoded-byte budget below the provider request ceiling: keep PNG when it fits,
  otherwise use adaptive JPEG compression and resizing before sending. Qwen
  vision uses the catalog's 512-token small-image ceiling,
  `reasoning_format: hidden`, the
  catalog-owned `reasoning_effort: none`, and the Groq-accepted subset of the
  provider-documented non-thinking sampling profile (`temperature: 0.7`,
  `top_p: 0.8`, `presence_penalty: 1.5`). `top_k` and `min_p` must remain
  absent because the current Groq endpoint rejects each field with HTTP 400.
  It also uses a conservative local
  preflight for the portable 8,000-TPM tier. Prompts that cannot leave the
  fixture-owned image/envelope reserve fail before image encoding or network
  I/O. Other Groq vision models leave the ceiling unset. Blank final content
  advances the normal model fallback chain; it does not retry the same model
  with a different reasoning policy. A token-rate 429 may retry once when
  Groq's structural `retry-after` is at most two seconds; otherwise preserve the
  provider error and continue the normal fallback chain. The short ceiling
  prevents long quota-window responses from freezing the feature. Windows and
  Android use the same contract.
- NVIDIA ordinary text, refine, and vision requests use deterministic
  non-thinking sampling (`temperature: 0`) unless a newer signed feed supplies a
  compatible endpoint control. Feed controls are applied by endpoint identity,
  never by display name.
- The canonical general image retry order comes only from the catalog priority
  chain. Availability is a hard gate: a provider failure that blocks fallback
  for tens of seconds outweighs a fast successful-call median when ordering the
  chain. The shipped image chain is bounded to ten entries and the shipped text
  chain to twelve entries on both platforms; generated defaults and the shared
  retry fixture must preserve those bounds. Do not duplicate a prose or
  platform-specific model list.
- Computer-control pixel grounding has a separate catalog-owned fail-closed primary/fallback chain, locked by the Phone Control model-chain fixture. `CC_VISION_MODEL` explicitly replaces that default chain with one diagnostic model. General OCR/description fallbacks never inherit authority to click. A transport error, empty response, or malformed structured response may advance to the next grounding model; a valid not-visible or verification rejection is terminal. Coordinate clicks require a fresh marked-crop verification at 70% confidence; `CC_VERIFY_LOCATE=0` is a diagnostic escape hatch, not a preset default.
- Retry state is shared by all preset request paths in a process. A rate-limit
  response observes a provider-reported delay between five seconds and six hours
  or uses the five-minute default. Two consecutive timeouts open a thirty-minute
  circuit; unavailable and billing failures open a six-hour circuit. Open
  circuits become half-open after expiry and admit one probe. Success closes the
  circuit and clears failure state; a cancelled or abandoned probe releases its
  claim. Recorded token-budget reset metadata may defer a request before network
  I/O when the known remaining budget cannot admit it. Provider error bodies and
  structural `retry-after` headers remain in the classified failure instead of
  being reduced to a status code. Presentation streaming and transport streaming
  are separate: a final-only Markdown or raw-HTML result may still consume a
  streaming provider response internally for liveness without exposing partial
  content. Streaming HTTP calls have independent response-start and progress-idle
  deadlines and no whole-response deadline while bytes keep arriving.
  Non-streaming interactive calls use a structural hard budget derived from the
  encoded request size and the endpoint's output-token allowance, clamped from one
  to fifteen minutes. Catalog benchmark latency is presentation/ranking evidence;
  it never defines a request's whole-call deadline.
- OpenRouter ordinary text, refine, vision, and recorder-subtitle requests
  apply catalog reasoning policy through OpenRouter's nested
  `reasoning: { effort: "none" }` field. `reasoning_effort` is not an
  OpenRouter transport field. Unknown/custom OpenRouter models remain
  provider-managed unless they have an exact catalog profile.
- Hidden blocks execute without windows; each visible result block owns its own result window.
- Unsupported graph/provider paths return an explicit reason. Never guess from ID prefixes.

## Fixtures

- [audio-runtime.json](../../parity-fixtures/preset-system/audio-runtime.json)
- [catalog-overrides.json](../../parity-fixtures/preset-system/catalog-overrides.json)
- [custom-models-dialog.json](../../parity-fixtures/preset-system/custom-models-dialog.json)
- [gemini-live-socket-protocol.json](../../parity-fixtures/preset-system/gemini-live-socket-protocol.json)
- [node-graph-editor.json](../../parity-fixtures/preset-system/node-graph-editor.json)
- [result-overlay.json](../../parity-fixtures/preset-system/result-overlay.json)
- [Android WebView overlay rendering](../../parity-fixtures/android-webview-overlays/rendering-contract.json)
- [retry-runtime.json](../../parity-fixtures/preset-system/retry-runtime.json)
- [text-input-overlay.json](../../parity-fixtures/preset-system/text-input-overlay.json)
- [text-provider-routing.json](../../parity-fixtures/preset-system/text-provider-routing.json)
- [vision-payload.json](../../parity-fixtures/preset-system/vision-payload.json)
