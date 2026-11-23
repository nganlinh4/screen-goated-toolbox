# Preset System Parity

## Canonical Source

- Windows editor/catalog: [preset settings](../../src/gui/settings_ui/preset.rs), [preset model](../../src/config/preset/preset.rs)
- Windows execution: [chain](../../src/overlay/process/chain), [text input](../../src/overlay/text_input), [result](../../src/overlay/result), [favorite bubble](../../src/overlay/favorite_bubble)
- Shared model catalog: [catalog/model_catalog.json](../../catalog/model_catalog.json)
- Android preset model/runtime: [shared preset](../../mobile/shared/src/commonMain/kotlin/dev/screengoated/toolbox/mobile/shared/preset), [Android preset](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/preset), [overlay host](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset)

## Data and Editor Contract

- Selection capture invokes the platform copy action, never an arbitrary button
  identified by localized text. Android tries `ACTION_COPY` on the focused node,
  then a platform `android:id/copy` control. Existing application selections,
  including WebView selections, remain the clipboard source. The shared cases
  are in `parity-fixtures/preset-system/selection-copy.json`.

- Windows built-ins are canonical seed data. Android persists user overrides by preset ID; restore removes the override.
  `parity-fixtures/preset-system/default-presets.json` is generated from
  `get_default_presets()` (`src/config/preset/defaults/parity_tests.rs`; rerun
  with `SGT_WRITE_PARITY_FIXTURES=1`) and replayed against the Android built-ins
  by `DefaultPresetsParityTest`: ids, localized names, section order, preset
  flags, connections and every block field. Input adapters compare only their
  output behavior; hotkeys and window geometry stay Windows-only. Image, audio
  and input-adapter blocks do not stream by default, as in the Windows builder.
- Preset edits are published only after a complete atomic persistence write. An
  unreadable existing override document is preserved and must not be treated as
  an empty document for subsequent saves; the interface reports the failure.
  Failed edits leave the editor showing the last successfully saved state;
  prompt and name drafts remain open for retry until their save succeeds.
  The persistence cases live in `parity-fixtures/preset-system/persistence.json`.
- A custom preset has one user-chosen name, as on Windows. Renaming it writes
  that name for every UI language (English, Vietnamese and Korean), the same
  way create and duplicate do. The rename draft opens with the name shown in
  the current UI language. Built-in presets are not renamed in the editor.
- Concurrent edits merge only the fields changed by that editor action against
  the current preset. Delayed updates cannot restore a deleted preset, and
  newly created preset identities cannot replace an existing preset.
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
  The generic image default follows the first available reviewed catalog entry.
  Built-in image
  translation and accuracy-labeled OCR pipelines, structured tables, fact
  checking, comprehensive extraction, and ask-image all follow that broad
  stable default. Catalog availability or priority-chain membership does not
  make a model a built-in preset default. Authority-bearing Computer/Phone
  Control keeps its separate catalog-owned model chain.
- The fast text-arena seed uses Groq GPT-OSS 20B. Groq Qwen 3.8 leads the
  general image chain after returning all ten ordinary OCR cases on
  2026-11-23. Gemini 3.5 Flash Lite and Gemini 3 Flash are next. The text chain
  starts with Qwen 3.8, then Groq GPT-OSS 120B and 20B, and includes Particle-1
  and Ling 3.2 Flash as OpenRouter fallbacks. The shared retry fixture records the exact order
  and default IDs. Existing saved choices follow normal reset/adoption behavior.
- The Windows one-time post-update recommendation prompt compares the staged
  preset models, priority chains, and recommended-provider defaults. Applying
  updates changed built-in model slots, restores both priority chains, and
  enables every currently recommended provider without disabling any other
  provider. Skipping only consumes the marker. Android has no equivalent
  self-update staging hook: generated recommendations apply to clean/reset
  runtime settings, while persisted Android choices remain unchanged until the
  user resets them.
- Favorite state, ordering, edits, and custom presets are repository-backed, never Compose-only state.
- Deleting a preset preserves the selected surviving preset. If the selected
  preset is deleted, select its previous neighbor, or the first remaining row;
  an empty list has no selected preset. Selection follows the structural row,
  including when saved preset IDs are duplicated. Windows cancels shortcut
  capture when its target is deleted or the active profile changes, and keeps
  capture bound to the same surviving row when earlier rows are deleted. The
  shared cases are in `parity-fixtures/preset-system/list-identity.json`.
- Android supports preset creation, duplication, deletion, and the current node-graph editor actions. Capability UI must still reflect real runtime support for every block/provider.
- Node-graph saves retain every authored block, including disconnected drafts,
  and their connections. Execution starts at the input and does not run
  disconnected blocks. A persisted explicit-graph marker distinguishes an
  intentionally empty edge set from an older linear preset without edges;
  absent markers preserve legacy behavior. Capability and audio-runtime
  selection use only input-reachable blocks, so an unreachable draft cannot
  start or block a runtime. Opening a graph with malformed or
  unrepresentable connections must not panic or rewrite the saved graph.
  Its graph controls are read-only with a visible explanation until an
  explicit restore or repair replaces that data; unrelated preset settings may
  still be saved without changing its blocks or connections. Shared cases are
  in `parity-fixtures/preset-system/node-graph-editor.json`.
- Inline node chrome (model and prompt labels, add-language button, the
  empty-prompt placeholder, the language search hint, and the
  `Stream on` / `Stream off` toggle) uses the Windows localized labels.
- Connecting a new source to an occupied input replaces its existing source;
  self-connections and unsupported target types remain rejected.
- New presets and newly inserted graph nodes resolve their modality default
  through the shared catalog constants; platform editors must not carry a
  literal fallback model ID.
- A saved model ID remains unchanged through loading, publication,
  unrelated edits, and saves even when its model has not yet been discovered
  or has the wrong modality, including a blank or whitespace-only ID. An
  unresolved or incompatible pinned model cannot
  dispatch or silently enter a different provider's retry chain: show an
  unavailable state and let the user retry after discovery or change the model.
  Recheck the selected model against the current catalog before execution so
  a later discovery can restore it without editing the preset. This applies to
  text, image, and audio blocks. Normal retry after an actual known-model
  failure keeps its existing configured-chain behavior.
- Authored priority chains retain unresolved nonempty IDs and their relative
  order. An unresolved candidate is skipped at execution until it resolves;
  merely loading or saving settings must not replace it with a default.
  [saved-model-resolution.json](../../parity-fixtures/preset-system/saved-model-resolution.json)
  records the shared state transitions.
- Ordinary image and text blocks can opt into a global first-attempt override
  by reordering the chosen-model row in their priority chain. The Windows
  executor and Android graph executor validate the saved selection before
  applying the order. Candidate checks, cancellation and automatic fallback
  still apply at dispatch. Dedicated non-LLM blocks keep their own route.
- Unknown/corrupt override fields fall back safely to canonical built-ins.
- Hotkeys and controller/master invocation remain Windows-only until Android has a real equivalent.

## Launch and Overlay Contract

- Presets execute from the floating bubble service. Zero favorites shows a localized empty state.
- The panel preserves Windows keep-open, size, multi-column, overlap, animation, drag/reposition, and refresh semantics through a thin Android bridge.
- Input uses the Windows text-input DOM/CSS/message contract, including submit, cancel, history, close, and working microphone input.
- Text-input documents bind messages and edits to their owning invocation. Submit
  carries the rendered revision;continuous-input cleanup clears only that submitted
  revision,so newer typing survives a delayed clear. A retired document cannot
  submit,move,focus,or close its replacement. Preference changes update chrome and
  theme without replacing the draft or selection. Android imports the canonical
  Windows markup,styles,and typed edit scripts through the overlay asset generator;
  touch dragging,IME focus,and native window lifetime remain platform adapters.
- Permission-gated image/audio paths fail before capture, explain the required Android permission, and preserve retry state.
- Finite audio recordings stop and submit at the ten-minute capture limit with a visible notice, retaining all accepted audio. Continuous transcription may continue beyond that limit, but omits the entire audio attachment rather than presenting a partial recording. Android uses temporary storage while capturing and releases it when the session ends.
- If live transcription fails while a complete recording remains available, capture falls back to a finite recording; if the recording has already been omitted, capture fails promptly instead of continuing without a recoverable result. A live setup backlog may not silently discard input.
- Android audio input previews read the current result's owned audio through a per-window capability, with seeking and native download; replacement, navigation, and closure revoke access. The result document does not contain an encoded copy of the recording.
- Android audio permission recovery uses a non-exported activity and resumes only the exact pending capture request after permission succeeds; exported launcher intents cannot start or resume capture. A stale or denied recovery never starts another session. The request belongs to its overlay controller and original input window, if any; stop, replacement, or input closure invalidates its continuation, and dictation returns to that same input window. A delayed permission result may only reach an already-live owner and cannot restart the overlay service.
- Image presets support continuous relaunch. Holding a text-input preset opens a persistent editor: submitting clears and refocuses it without changing the saved preset. Holding a selected-text preset retains the canonical Windows selection badge; tapping captures a fresh selection, and Stop disarms further captures. Android uses explicit touch capture because desktop mouse-release monitoring has no equivalent. Each callback remains bound to the active invocation; a stopped or replaced invocation cannot submit late clipboard text.
- Continuous image capture has one owning invocation. Stop during startup disarms it, startup failures leave capture inactive, and a replacement starts after the previous invocation has released its capture resources. A stale invocation cannot retire or publish input for its replacement. Desktop right-button hooks and their message queue are platform-specific; Android uses its own touch and projection lifecycle. Shared ownership cases are in `parity-fixtures/preset-system/continuous-image-lifecycle.json`.
- Android fences frame, screenshot and selection callbacks to their invocation. A replacement waits for an already-requested screenshot and for an in-flight overlay show and detachment; stale screenshot bitmaps are recycled. If detachment fails, the old overlay remains owned and its Cancel action retries closure without submitting stale input. A screenshot callback or detachment that never settles can block a replacement; device acceptance remains unverified.
- Result windows are session-owned, precreated in loading state, multi-window, and support markdown streaming or raw HTML according to block render mode.
- A replacement or canceled invocation retires its prior session before publishing
  new execution state. Late window chunks, errors, completion, and post-processing
  from the retired session cannot mutate the replacement or perform effects.
- Deferred copy, paste, and speech actions belong to the session that scheduled
  them. Completion keeps them valid until replacement, cancellation, or reset;
  retired sessions cannot dispatch queued actions or speech retries.
  Buffered speech stops promptly when its session retires, including while
  waiting for output capacity or for playback to drain. A drain timeout is a
  partial playback outcome and does not replay speech that may have been heard.
  A speech response that produces no playable audio is a failure, so it can
  follow the normal retry or error path rather than reporting completion.
  Buffered time-stretch output is finalized for the current speech request and
  cleared on termination; samples cannot carry over to a later request.
- A model recovery probe belongs to the attempt that claimed it; cancellation
  or completion of another attempt cannot release or resolve that probe.
- Raw HTML is a document lifecycle, not a Markdown DOM update. A complete raw
  document (including one harmless outer `html`/`htm` code fence) is normalized,
  loaded once, and remains authoritative: the host must never replace its body
  through the generic Markdown `innerHTML` updater after load. Authored CSS,
  animations, scripts, and interaction state remain owned by that document;
  host integration is limited to overlay interaction, navigation, diagnostics,
  visibility, and lifecycle bridges.
- Whether a block streams is decided by its render mode only, never by prompt
  wording. A streamed or refined result that becomes a complete raw document
  is loaded once after its stream ends; a document identical to the loaded one
  is never reloaded.
- Blocks run as a dataflow (Windows `post_process.rs`): a block starts when
  its parents finish, and independent sibling branches run concurrently. A
  block whose running parents all failed, produced blank text or were skipped
  is skipped and its window closed, unless a parent is the input adapter.
- A result window runs at most one refine. A new refine replaces the running
  one, and closing the window or dismissing all overlays cancels it; callbacks
  of a replaced refine are ignored. Standalone windows (no preset) do not offer
  refine.
- Raw-document detection requires an explicit, complete `html` root: a closed
  opening tag and a final closing tag, optionally preceded by an HTML doctype
  and enclosed in a single outer HTML fence. Until that root is complete,
  streaming text remains Markdown and then enters the raw-document lifecycle.
  Unambiguous HTML comments may surround the root or follow its doctype.
  This conservative rule leaves documents with implicit root closure or ambiguous
  script/comment parsing in Markdown; it prevents incomplete tags and standalone
  tag explanations from being run. Apparent closing tags inside unclosed
  foreign-content CDATA are not document boundaries. Single outer fences accept
  CR, LF, and CRLF line endings. SVG and MathML are tracked separately; markup
  inside their raw-text-like elements is accepted only when it is unambiguous.
  Self-closing raw-like foreign elements are recognized only before an untracked
  child or surrounding sensitive context can switch the parser's namespace;
  namespace ambiguity propagates through nested foreign elements.
- Interactive raw-HTML presets choose controls appropriate to the generated experience: keyboard, pointer, touch, or a combination. The built-in Make a Game prompt must not impose a mouse-only design; it requires visible control instructions and keyboard activation after the player clicks or taps the result.
- The result-control badge identifies the endpoint that actually produced the current result after retry/fallback, formatted as provider display name plus the complete API model name; non-model results have no badge.
- Android result and mini-app WebViews follow [the shared Android overlay rendering contract](../../parity-fixtures/android-webview-overlays/rendering-contract.json): each overlay window owns hardware acceleration and its WebView composes directly without a persistent offscreen hardware layer.
- Reuse Windows markdown fitting/theme/font/table and button-canvas contracts. Preserve text selection, one-finger window drag, two-finger bidirectional content scroll, navigation recovery, and result geometry ownership.
- Result tables use the same bundled, version-pinned Grid.js JavaScript and CSS
  as Windows; displaying a result does not download executable table code.
- Markdown fragments use the Windows result compositor's HTML allowlist before
  insertion into the privileged result shell. Raw HTML documents retain their
  separate interactive document lifecycle.
- Android requires an observed user gesture before an overlay may launch an
  external application for a non-WebView navigation scheme.
- Untrusted result documents and intercepted external navigations use a separate
  WebView without a page-script native bridge. The trusted shell uses an origin-bound main-frame
  bridge. When the WebView provider supports isolated script worlds, untrusted
  result gestures run in a dedicated world and accept only trusted touch and
  click events; messages from subframes are rejected. Without that capability,
  the document remains readable and navigable, with native result controls,
  while document-origin privileged page actions are unavailable.
- A native tap on an untrusted result reveals its result controls even when it
  is already active. Gestures from a navigated document stay disabled if its
  identity cannot be established; the native controls remain available.
- Android does not expose every navigation request to the interception callback.
  Any external document that remains in the original WebView is denied bridge
  messages and injected result state by the origin and document identity checks.
- Android input-adapter audio downloads are bound to the current result's native
  audio bytes and a trusted user action; page scripts cannot supply the bytes or
  destination. If isolated gesture support is unavailable, the embedded control
  reports failure and the native result control remains available.
- Edit/refine, undo/redo, share/download, and speaker actions are real Android actions. Refine starts with the endpoint that actually produced the displayed text when it is text-capable, then uses the current adaptive Text-to-Text retry chain and circuit policy. Its result badge follows the endpoint that completes the refine. Do not list implemented actions as placeholders.
- Android result controls expose the Markdown/plain toggle. Plain mode escapes the source text and preserves it through streaming, refinement, history navigation, and theme changes. Raw HTML documents keep their own lifecycle and do not expose the toggle. Broom mouse-button variants remain desktop-specific.
- Tablet acceptance exercises the real language picker and overlay WebViews in
  both distributions: language selection persists, continuous submissions clear
  and refocus without persisting the launch override, result toggles survive
  content/theme updates, and stopping continuous selection disarms later input.
  Reopening the same selection preset creates a new invocation; queued callbacks
  from the destroyed badge cannot capture or stop the replacement.

## Provider Contract

- OpenAI-compatible text and vision adapters preserve provider error events,
  including errors carried by an HTTP-success stream. Malformed data events,
  empty output, incomplete streams, and non-success completion reasons fail the
  attempt. A stream completes only with `[DONE]` or an explicit successful
  finish reason; unary replies reject explicit non-success finish reasons.
  Usage-only events and SSE comments do not count as output. After a successful
  finish, empty deltas may repeat the successful finish or carry metadata;
  further output, malformed deltas, errors, and non-success finishes still fail.
  The same reducer
  serves ordinary streaming text, refinement, and vision on both platforms.
  Partial output may be presented provisionally but is never accepted as a
  successful final result after a failed completion.
  Structural completion failures carry `PROVIDER_RESPONSE_INVALID` and advance
  the normal model fallback chain. Cancellation remains terminal and must not
  enter a buffered-reader retry loop.
  Quota preflight skips an endpoint only when its tracked balance is exhausted;
  it does not reserve guessed image or completion tokens for either modality.
  When Groq rejects an oversized output reservation with an explicit OTPM
  allowance, ordinary text and vision may retry once at that reported allowance. This shares the
  existing short-rate-limit retry slot. Unknown errors and exhausted quota use
  normal fallback; completion-limit responses never become successful OCR.
  The contract is covered by `parity-fixtures/preset-system/groq-output-allowance.json`.
  A provider rejection whose declared request size exceeds its token allowance
  is request-scoped (`PROVIDER_REQUEST_LIMIT`), not evidence of an unhealthy
  endpoint. It advances fallback without opening a model cooldown. Exhausted
  quota with a used balance remains rate-limited and honors the provider wait.

- Structured text callers explicitly choose provider-enforced schemas or
  locally validated streaming. The latter is permitted only when the caller
  validates each completed item and owns recovery for missing items. On Groq,
  that mode omits provider JSON mode while keeping deterministic sampling;
  unary calls retain provider enforcement. Ordinary presets keep their existing
  request behavior. Windows Screen Translate owns the locally validated mode;
  Android has no corresponding Screen Translate caller yet. Its Groq transport
  may retry an HTTP 429 once when structural `Retry-After` is at most two
  seconds, using the same cancellable policy as vision. Longer or absent delays
  continue through the normal model chain.
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
- Search capability preserves explicit-search retry compatibility and draws
  the model-list marker. The separate catalog-owned
  `search_tool_enabled_by_default` flag controls normal request behavior. Ordinary
  text/audio generation never invokes provider search tools implicitly; an initially
  selected endpoint that enables search by default requires explicit opt-in;
  quota-bearing grounding is enabled only by an explicit search feature path.
  Groq GPT-OSS search uses `browser_search` with a required tool choice and
  rejects structured output before dispatch on both platforms.
- The request's explicit search setting stays unchanged across retries. A
  search-capable failed model alone does not require its replacement to support
  search; ordinary requests skip models that invoke a search tool by default.
  Explicit search retries use only endpoints whose actual adapter supports the
  requested tool, and fail if none can be used. Image-block search fails before
  any provider call while image adapters cannot carry that request.
  Shared retry cases live in `parity-fixtures/preset-system/retry-runtime.json`.
- Gemini Live setup uses the catalog-owned output ceiling for each endpoint: 8,192 for Live 2.5 and 65,536 for Live 3.1, on both Windows and Android.
- `Viết liên tục` uses the older Live 2.5 input-transcription row
  (`google-gemini-2-5-live-transcribe-audio`), rendered as `GG Live cũ (Chép)`
  in Vietnamese. The custom-prompt row sharing that endpoint remains
  `GG Live cũ`.
- Every native Live setup envelope is built through the platform's typed setup builder; endpoint policy is applied by construction and feature adapters supply only capability deltas.
- Live server events are decoded structurally. Setup completion must be a top-level field, all audio parts in a frame are retained, and finite responses complete on either `turnComplete` or `generationComplete`.
- Finite Gemini Live text/vision response waits remain cancellable before and after first output. Cancelling closes the owning socket and prevents any subsequent chunk delivery; an empty frame does not count as first output. Setup errors take precedence over acknowledgement.
- Every finite response owns its queued work and accepted transport until completion,
  failure,cancellation,or caller abandonment. Receive and socket-poll boundaries
  recheck retirement after blocking. Empty chunks do not start output or refresh an
  idle deadline;wire traffic without output cannot keep the first-output wait alive.
  [live-response-wait.json](../../parity-fixtures/preset-system/live-response-wait.json)
  locks the caller poll budget and ordinary/interaction-idle deadline transitions.
- Blank, legacy, or unknown Gemini TTS model values normalize to the catalog-owned TTS default on both platforms; listed models remain unchanged.
- Provider/auth failures and retryable model failures remain distinct. Retrying an open result updates its loading status.
- Every enabled provider has a complete encrypted credential path from settings
  state to execution. In particular, NVIDIA availability and requests use the
  persisted NVIDIA key on both platforms; an empty key makes that provider
  unavailable without altering the authored chain.
- A non-streaming plain-text vision caller never requests JSON mode or a schema.
  Endpoint profiles that may restate the requested output use the same
  endpoint-scoped repetition guard on both platforms; unrelated endpoints and
  structured callers are unchanged. The catalog owns which endpoints opt in.
  Matching reuses exact non-overlapping spans within the bounded scan instead of
  repeating substring searches for every candidate. Thresholds and salvage
  decisions stay identical. Shared long-output cases live in
  `parity-fixtures/preset-system/vision-repetition-work.json`.
- Ordinary LLM vision request shape comes from
  `catalog/model_catalog.json#vision_request_profiles` on both platforms.
  Google vision endpoints send image before text; Groq Qwen sends text before
  image. Media resolution remains provider-default:
  small-image probes showed no durable completion-latency win from forcing a
  lower setting. Plain OCR uses progress streaming when the endpoint supports
  it, with interactive first-output deadlines, even when presentation waits for
  the complete transcription.
- A caller-supplied vision schema is sent only when the endpoint profile declares
  strict structured-output support. Gemini uses `responseJsonSchema`; endpoints
  without that capability keep their catalog-owned prompt-only or JSON-object
  transport. Plain OCR supplies no schema.
- Vision payloads preserve their real MIME type. Groq images use a prompt-aware
  encoded-byte budget below the provider request ceiling: keep PNG when it fits,
  otherwise use adaptive JPEG compression and resizing before sending. Qwen
  vision uses provider-native output limits,
  `reasoning_format: hidden`, the
  catalog-owned `reasoning_effort: none`, and the Groq-accepted subset of the
  provider-documented non-thinking sampling profile (`temperature: 0.7`,
  `top_p: 0.8`, `presence_penalty: 1.5`). `top_k` and `min_p` must remain
  absent because the current Groq endpoint rejects each field with HTTP 400.
  Account-tier token admission is provider-owned; a guessed portable TPM budget
  never rejects an otherwise valid request locally. Blank final content
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
  or uses the five-minute default. The first two consecutive timeout failures each
  open a fifteen-second circuit. The third and subsequent timeout failures open a
  sixty-second circuit, capped there. Success resets the timeout sequence. HTTP 5xx
  and invalid provider responses open a fifteen-second circuit, except an output token limit (which still
  advances the current chain but is request-dependent). This health state skips
  requests before network I/O without changing the configured order or using
  image dimensions. Unavailable and billing failures open a six-hour circuit. Open
  circuits become half-open after expiry and admit one probe. Success closes the
  circuit and clears failure state; a cancelled or abandoned probe releases its
  claim. Interactive preset chains own retries: adapters return HTTP failures to
  the chain immediately without sleeping and retrying the same endpoint. Android
  already dispatches one HTTP attempt; Windows scopes this ownership to synchronous
  chain execution so standalone provider calls retain their existing retry policy.
  Recorded token-budget reset metadata may defer a request before network
  I/O only when the provider-reported token balance is exhausted. Output ceilings
  and guessed image-token costs must not bench an otherwise usable endpoint. Provider error bodies and
  structural `retry-after` headers remain in the classified failure instead of
  being reduced to a status code. Presentation streaming and transport streaming
  are separate: a final-only Markdown or raw-HTML result may still consume a
  streaming provider response internally for liveness without exposing partial
  content. Every interactive HTTP call uses catalog full-completion latency as a
  bounded deadline baseline, with encoded request bytes contributing only a small
  upload allowance. Connect, send, response-start, first-token, and unary attempt
  deadlines follow the shared retry fixture. Streaming waits for the first
  nonempty output token using `clamp(2 × catalog latency, 3000ms, 8000ms)`.
  Headers, keepalives, and thinking do not satisfy that deadline. After output
  begins, neither an idle nor whole-response deadline interrupts generation;
  cancellation remains active. Unary responses retain bounded completion.
  Every dispatched attempt gets a fresh initial budget; earlier attempts never
  consume a later candidate's budget. There is no separate attempt cap or
  primary-model whole-chain deadline. DNS, TLS, connect, and request-send
  failures block sibling endpoints from that provider for the current request;
  response-start, first-token, and unary attempt timeouts remain model-specific.
  Retryable failures continue until a model succeeds, the user cancels, a terminal
  error occurs, or all compatible unique candidates are exhausted. Request or
  image size may adjust time allowance but never model eligibility or ordering.
  Blank final content is a retryable model failure.
- Ordinary vision uses provider-native output limits by default. A catalog ceiling
  is optional and must represent an intentional endpoint contract, not an OCR
  length estimate. Gemini success requires explicit `STOP` plus nonblank text;
  malformed events, provider errors, blocked responses, incomplete EOF, and
  `MAX_TOKENS` are failures even after partial text. Usage-only trailing metadata
  is accepted. Output-limit failures advance without opening a health cooldown.
- A catalog-owned minimum image dimension is a wire compatibility constraint.
  Proportional resampling may satisfy it without changing the selected model or
  chain order. Inputs already within the supported dimensions remain unchanged.
  An impossible aspect ratio fails only that request, not the endpoint's health.
- OpenRouter ordinary text, refine, vision, and recorder-subtitle requests
  apply catalog reasoning policy through OpenRouter's nested
  `reasoning: { effort: "none" }` field. `reasoning_effort` is not an
  OpenRouter transport field. Unknown/custom OpenRouter models remain
  provider-managed unless they have an exact catalog profile.
- Hidden blocks execute without windows; each visible result block owns its own result window.
- Input-adapter media results keep their native media identity for result actions.
  Android copies an image through a content URI, offers audio through its native
  download action, and does not copy, speak, refine, or share generated media
  HTML as text. Clipboard and share failures are reported; oversized text is
  rejected rather than sent through an unbounded Binder transaction. Ordinary
  text result actions retain their existing behavior. These Android platform
  adaptations and their content-replacement cases are recorded in
  `parity-fixtures/preset-system/result-media-actions.json`.
- Android image clipboard files are immutable per publication. Files whose
  publication may have succeeded remain available, including after an error;
  unpublished files are removed. A bounded cache rejects further copies when
  its capacity of 64 files or 128 MiB is exhausted rather than invalidating an
  earlier clipboard URI. Copy then reports failure until cache space is made
  available; no background clipboard read can reliably establish grant expiry.
- Unsupported graph/provider paths return an explicit reason. Never guess from ID prefixes.

## Fixtures

- [audio-runtime.json](../../parity-fixtures/preset-system/audio-runtime.json)
- [catalog-overrides.json](../../parity-fixtures/preset-system/catalog-overrides.json)
- [chat-completion.json](../../parity-fixtures/preset-system/chat-completion.json)
- [custom-models-dialog.json](../../parity-fixtures/preset-system/custom-models-dialog.json)
- [gemini-live-socket-protocol.json](../../parity-fixtures/preset-system/gemini-live-socket-protocol.json)
- [node-graph-editor.json](../../parity-fixtures/preset-system/node-graph-editor.json)
- [result-overlay.json](../../parity-fixtures/preset-system/result-overlay.json)
- [Android WebView overlay rendering](../../parity-fixtures/android-webview-overlays/rendering-contract.json)
- [retry-runtime.json](../../parity-fixtures/preset-system/retry-runtime.json)
- [text-input-overlay.json](../../parity-fixtures/preset-system/text-input-overlay.json)
- [text-provider-routing.json](../../parity-fixtures/preset-system/text-provider-routing.json)
- [vision-payload.json](../../parity-fixtures/preset-system/vision-payload.json)
- [gemini-completion.json](../../parity-fixtures/preset-system/gemini-completion.json)
