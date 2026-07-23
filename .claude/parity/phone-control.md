# Phone Control Parity Contract

Status: implementation in progress. Emulator evidence is recorded only after each
contract layer is exercised; untested real-device authority remains explicitly open.

Research baseline: 2026-07-18. Re-check the linked Android, Play, Shizuku, Chrome,
and ONNX Runtime documentation before implementation because platform and store
rules change independently of SGT.

## Canonical Source

- Product name on Android: **Phone Control**.
- Windows Computer Control is canonical:
  - `docs/COMPUTER_CONTROL_DEVELOPMENT.md`
  - `src/overlay/computer_control/mod.rs`
  - `src/overlay/computer_control/protocol.rs`
  - `src/overlay/computer_control/uia_task.rs`
  - `src/overlay/computer_control/uia_task/prompt_core.txt`
  - `src/overlay/computer_control/uia_task/prompt.rs`
  - `src/overlay/computer_control/runtime/session_control.rs`
  - `src/overlay/computer_control/browser/mod.rs`
  - `src/overlay/computer_control/artifacts.rs`
  - `src/overlay/computer_control/memory.rs`
  - `src/overlay/computer_control/research.rs`
  - `src/overlay/computer_control/mcp/mod.rs`
  - `src/overlay/computer_control/system_query/mod.rs`
  - `src/overlay/computer_control/detector.rs`
  - `src/overlay/computer_control/telemetry.rs`
- Canonical end-to-end evaluation:
  - `tests/COMPUTER_CONTROL_GOLDEN_SUITE.md`
  - `tests/computer_control_golden_suite.json`
  - `tests/VISION_GROUNDING_BENCHMARK.md`
- Shared live-session contract:
  - `.claude/parity/gemini-live-session.md`
  - `parity-fixtures/gemini-live-session/lifecycle.json`
  - `mobile/shared/src/commonMain/kotlin/dev/screengoated/toolbox/mobile/shared/live/GeminiLiveLifecycle.kt`
- Existing Android foundations to extend, not replace:
  - `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/SgtAccessibilityService.kt`
  - `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/SgtAccessibilityTextInjectionSupport.kt`
  - `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/shared/live/GeminiLiveProtocol.kt`
  - `mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/shared/live/GeminiLiveReadySession.kt`
  - `mobile/androidApp/src/main/res/xml/accessibility_service_config.xml`
  - `mobile/androidApp/src/main/AndroidManifest.xml`

There is no canonical web surface to port. Port the Windows agent, turn,
capability, evidence, and effect contracts. Android UI is a thin native shell
around those contracts.

## Product Scope

- Phone Control is one product in both `play` and `full` distributions. The entry
  point, stable catalog, runtime, Accessibility backend, Shizuku/root clients,
  setup flow, lifecycle, and typed results must stay behaviorally identical.
- Distribution may change only delivery mechanics for large offline assets. Use
  Play Asset Delivery/dynamic-feature splits where appropriate; a missing module
  is a typed provider state and never a smaller catalog or weaker agent.
- Store listing and policy compliance are release-review concerns, not runtime
  phrase gates or source-set feature removal. Keep policy declarations and user
  disclosures truthful without changing requested semantics.
- The goal is the strongest control Android permits after explicit user grants.
  It is not a promise to bypass the lock screen, secure surfaces, hardware-backed
  authentication, OS-owned confirmations, SELinux, or unavailable OEM APIs.
- Accessibility is the baseline semantic/control backend. Shizuku, root, device
  owner, direct app APIs, browser debugging, notification access, media
  projection, and future privileged deployments are optional capability
  providers, not alternate agent designs.
- Android's developer-verification rollout can affect how a full APK is
  installed, but it grants no runtime authority and weakens none of the setup
  checks. Track the current
  [developer-verification guidance](https://developer.android.com/developer-verification/guides/faq?hl=en)
  in release planning.

Before each Play release, review the current
[Accessibility automation policy](https://support.google.com/googleplay/android-developer/answer/10964491?hl=en),
complete the applicable Accessibility declaration, and ensure the product's
store description, prominent disclosure, consent, and data handling match the
implemented assistant behavior. Policy uncertainty must be surfaced for release
review; it must not silently fork Phone Control into a weaker Play implementation.

## Prime Contract

- The live model owns language meaning, planning, tool choice, and semantic
  completion.
- Every normal turn receives one stable, full Phone Control tool catalog.
- Code gates only job identity, cancellation, stale targets, required structured
  fields, consequential checkpoints, postconditions, reconnect/audio safety,
  and typed repeat-failure limits.
- Never add phrase, keyword, language, app, site, person, task, model-run, or OEM
  permission logic.
- Never silently replace a requested tool with another tool. A provider router
  may choose the highest-fidelity implementation of the same capability and
  must report which provider ran.
- Each implemented tool declares the exact provider subset its handler can use.
  That subset must preserve the capability route's order. The dispatch boundary
  builds this structural plan but does not pre-gate an implemented handler from
  a registry snapshot: composite handlers make the freshest provider-specific
  probe, execute only a planned provider, and report the provider that actually
  ran. Successful primary receipts must report `ready`; successful or possibly
  effectful receipts must name a provider inside the plan. A non-success receipt
  preserves its honest degraded, revoked, or user-step provider state and effect
  certainty. Secondary evidence such as `input_provider` names a dependency and
  is not mistaken for the primary route. A mismatch is a typed internal
  `provider_contract_failure` that preserves honest effect certainty. Only a
  nonmutating, proven-no-effect `tool_contract` failure bypasses provider
  attestation. A proven-no-effect dependency failure may name the provider that
  rejected the attempt. An unexpected exception reports an unattributed provider
  failure; it never guesses the tool's first provider. A dependency may return a
  proven-no-effect failure only when that provider is declared on the exact tool
  and the receipt explicitly sets `provider_role=dependency`; all other receipts
  must attest their primary provider against the exact execution plan.
- Unavailable tools stay declared. They return a typed capability result that
  names the missing grant or backend and any required user step.
- Unknown future integrations remain available by default through the same
  catalog and capability registry.
- One user turn produces at most one final response, then settles idle.

`parity-fixtures/phone-control/authority-matrix.json` is the machine-readable
authority and routing contract. Catalog availability is never inferred from
the user's words.

Pixel grounding uses the catalog-owned ordered model chain and fallback contract
in `parity-fixtures/phone-control/model-chain.json`. Windows Computer Control and
Android Phone Control consume the same generated IDs; neither platform owns a
second locator-model constant.

## Behavior Contract

### User-visible flow

1. Phone Control lives directly on the Apps card in both APK distributions.
   The card has the same compact **Turn on / Turn off** interaction as Live
   Translate and sits directly beside it in the app carousel; it does not open a
   Phone Control page.
2. **Turn on** runs one structural activation coordinator. It probes current
   evidence and opens only the next missing app-owned permission request or
   Android-owned settings surface. On return it re-probes and continues. It
   never shows a capability checklist, setup dashboard, or user-facing self-test.
3. The required activation path is Gemini API configuration, microphone,
   foreground notification where Android exposes that runtime permission,
   Accessibility, and display-over-other-apps for the orb. When the key is
   missing, the coordinator shows one short toast and opens SGT's existing
   Settings section, where provider credentials already live. It does not own a
   second credential form. Android grants stay in Android-owned surfaces. A
   refusal or unresolved grant stops that activation attempt without looping or
   claiming success, and another **Turn on** starts from fresh evidence.
4. Once required evidence is sufficient, SGT starts the foreground service and
   orb immediately. The card becomes **Turn off** and remains the primary stop
   control; the ongoing notification also retains a Stop action.
5. The first orb appearance asks for an optional power preference in a compact,
   orb-owned prompt: standard Android, Shizuku, or root. Standard control starts
   without an elevated provider. Choosing Shizuku immediately explains the next
   user-owned step, then the coordinator probes and advances the strongest safe
   setup route: request SGT's Shizuku grant when its Binder is ready, open the
   installed Shizuku manager when its service is stopped or authorization was
   revoked, or open the store listing with an official-download fallback when it
   is absent or outdated. Each return is re-probed and may advance only to a different
   capability state, so install -> start/pair -> authorize can continue without a
   setup dashboard or an external-surface loop. Android-owned wireless-debugging
   pairing, trust, and confirmation remain user actions. Choosing any elevated
   tier launches only its exact user-owned setup/authorization path. Tapping the
   orb reopens this small preference prompt, so setup remains reachable without
   an inner product page.
6. Capability checks and reversible self-tests remain internal diagnostics and
   acceptance seams. They never block the card with a wall of text. The orb then
   runs the same listen/work/respond/idle cycle as Windows Computer Control.

The wizard should automate navigation and diagnosis after Accessibility is
available. It must not hard-code settings coordinates, localized labels, or OEM
screen sequences. Use intents where Android exposes them, then the same
semantic/vision control stack as ordinary Phone Control.

Modern Android deliberately ignores the package URI on the public overlay
permission intent. After Accessibility is connected, the coordinator may
therefore scroll only the resolved Android Settings surface and open the unique
row whose text exactly matches SGT's runtime application label. This seam must
refuse ambiguous matches and checkable/toggle ancestors. It can expose the
app-specific permission screen, but it never toggles or grants permission. Before
Accessibility is connected, the public Accessibility settings surface remains
the platform boundary; the user still owns its service-selection and approval
steps. After the requested grant becomes observable, the coordinator may issue
bounded Back navigation only while that same resolved Settings package remains
foreground, returning control to the waiting activation activity.

The diagnostic journal records the coordinator open, each structurally selected
step, user-step presentation and return, Settings app-row opening, observed grant,
bounded return, service start acceptance, and terminal runtime state. These are
content-free receipts: API keys, transcripts, labels, page content, and Android
Settings text never enter them.

The activation transition table is machine-readable in
`parity-fixtures/phone-control/activation-flow.json`. It is capability based,
not phrase based, and is identical in Play and Full.

The reversible control check has a dedicated non-catalog seam: it may apply only
Accessibility focus followed by clear-focus to an eligible node on SGT's exact
current application surface. Selection uses the application window type and
exact runtime package, is bound to the current observation, and never depends on
labels or coordinates. Success requires both the focus transition and restored
state to be observed. This seam itself permits no click, text, key, gesture,
global-action, or command dispatch.

`controller-owned` means an actual Phone Control overlay window, never every
window that happens to share SGT's package. Accessibility overlays and SGT-owned
non-application fallback overlays remain excluded from observation and ordinary
tool dispatch. Normal SGT application windows are ordinary targetable surfaces
and keep the same stale-target, authority, confirmation, and postcondition rules
as any other application.

The optional browser-power step detects the preferred browser and standard
Custom Tabs support, then probes the enabled Shizuku/root/first-party bridge for
credentialed CDP targets. It explains that browser sign-in remains inside the
browser. SGT never asks the user to repeat that sign-in in an owned WebView or
offers to import browser data. Missing CDP authority does not block setup:
authenticated Custom Tabs plus Accessibility remain an honest degraded route.

Android 13 and later can require the user to allow restricted settings for a
sideloaded app before Accessibility can be enabled. Treat this as a typed
`needs_user_step`; do not loop or claim success. See
[restricted settings](https://support.google.com/android/answer/12623953?hl=en).

### State and transition rules

- Reuse the shared `agentSession` transport/lifecycle reducer. Do not create a
  Phone Control socket state machine.
- One active turn owns its model generation, tool jobs, tabs, captures, target
  snapshots, audio, and cleanup.
- At most one tool job is admitted session-wide. A later call is not executed or
  queued for execution; it receives a typed proven-no-effect rejection only
  after the active job reaches a terminal receipt. A cancelling job keeps that
  slot until its terminal cancellation acknowledgement is drained, so a new
  turn cannot race an effect that may still be settling.
- Every admitted job has one exact operation identity derived from turn,
  response generation, and job ID. Cancellation is addressed only to that
  identity. A provisional cancellation request never proves terminality and
  never releases the session slot. Cancellation handlers run on owned IO work,
  never inline on the lifecycle thread; their success or failure must settle
  before the terminal receipt is emitted.
- An owned provider boundary distinguishes three terminal outcomes: no owned
  boundary observed is unknown for a mutator; an observed boundary with no
  platform acceptance is proven no effect; platform acceptance is may-have-
  occurred until the provider reports its real terminal signal. The executor
  emits the cancellation receipt only after all owned effects are terminal.
- Held rejections contain only bounded response identity metadata. Overflow
  latches the logical generation, admits no more work, and waits for the owner
  terminal receipt. It then suppresses terminal `done`, abandons the session,
  clears its resumption handle and outbound payloads, and reconnects fresh;
  overflowed work and responses are never replayed into the new session.
- Tool-call frames are structurally preflighted before dispatch or rejection
  production: at most 33 calls, 1,024 UTF-8 bytes each for ID and name, 1 MiB
  for one arguments value, and 2 MiB across the frame. Exceeding a bound aborts
  that protocol session; code never interprets tool meaning to make this choice.
- Terminal tool delivery is one callback per operation token through a
  capacity-one completion slot. The outbound logical-session FIFO holds at most
  34 payloads, 48 MiB total, and 32 MiB for one payload. A rejected transport
  send leaves only its FIFO head pending. That FIFO survives reconnect only
  when a bounded, nonblank resumption handle is sent in the next setup; without
  one, old control payloads, screen/audio input, output, and generation state
  are abandoned before a fresh session is bound.
- A synchronous function call is answered before any tool-owned screen evidence
  or ambient screen frame is sent. Ambient video remains paused while that call
  is unanswered; microphone audio remains live so barge-in still works. Once the
  response is accepted by the socket, exact tool evidence follows in the same
  logical-session FIFO before ambient video resumes. Transport diagnostics keep
  only a bounded structural tail (payload kind, byte count, protocol phase, and
  pending-work count); they never retain payload content.
- `done` is terminal. Accept it once, release its current-generation audio once,
  retire later tools/output, run silent local cleanup, and return idle.
- If a generation ends without `done`, release its one response and return idle.
  Never synthesize a continuation, verifier turn, or completion quorum.
- Barge-in monotonically cancels only the owned turn/generation/jobs. Late
  callbacks from retired generations cannot act, speak, reopen the turn, or
  resurrect a reconnect.
- Interrupted mutations with no proven no-effect receipt block later mutation
  and completion until a fresh observation reconciles state.
- A mutation-requested screen refresh clears that reconciliation gate only after
  the fresh frame is successfully transmitted. It may release a generation whose
  completion was already deferred, but it never completes an active generation
  early and an unsent capture proves nothing.
- `done` and server generation completion cannot publish idle while the admitted
  job or its cancellation acknowledgement is pending. A completed `done` is
  delivered last after any held rejection receipts; it never cancels work to
  manufacture terminality or absorbs an effect after idle.
- Cleanup cannot generate model output or speech.

The Phone Control-specific cases are in
`parity-fixtures/phone-control/turn-contract.json`; socket-level cases remain
owned by `parity-fixtures/gemini-live-session/lifecycle.json`.

The debug device probe is a transport into this same production executor and
single-flight cancellation contract, not a second dispatcher. Its exported
debug receiver requires Android's `DUMP` permission. A cancel arriving before
tool-job attachment is delivered immediately after attachment; admission stays
owned through the production terminal completion, and atomic receipt suppression
prevents a late callback from recreating a cancelled probe receipt.

### Speech and captions

- Caption and audio belong to the same generation.
- Stream current-generation PCM as it arrives. Only a bounded device-startup
  buffer or a temporarily missing output sink may delay playback.
- Tool dispatch, tool completion, semantic completion, and postcondition checks
  never gate current speech.
- Dropped or interrupted generation audio never plays later.
- Turn cleanup and capability diagnostics are silent unless the current model
  explicitly includes them in its single response.

### Orb surface

- `src/overlay/computer_control/orb/orb.html` is the single renderer for the
  Windows Computer Control orb and Android Phone Control orb. Android stages the
  byte-identical asset at build time and adds only a platform bridge, local
  placement, and touch shim.
- State labels, liquid-body palettes and motion, glow, embedded Material Symbols
  path data, icon loops, and directional scroll overrides come from the canonical
  renderer and `parity-fixtures/phone-control/orb-contract.json`; Android must not
  redraw a substitute circle or maintain a second icon catalog.
- Android renders the caption in that same canonical HTML, including Google Sans
  Flex, transparent background, white text/shadows, placement, and incremental
  word motion. It must not maintain a native substitute caption or restart
  unchanged words as streaming text grows.
- The canonical visual renderer is a full-display, non-touchable trusted
  accessibility overlay; a separate orb-sized touch shim owns drag/tap input.
  This preserves exact pixels while underlying apps remain interactive. During
  an accessibility-service reconnect, an application-overlay fallback may render
  only at the platform-reported non-obscuring alpha until the trusted host returns.
- Crossing the touch shim's drag threshold opens the same shared, single-target
  bottom dismiss bubble used by Android's other floating overlays. Current raw
  pointer coordinates drive its proximity feedback. Releasing inside its commit
  threshold runs the canonical orb exit plus the shared swallow animation, stops
  the Phone Control foreground service, and returns the Apps card to **Turn on**.
  Releasing elsewhere hides the target and persists the clamped orb position;
  cancellation hides the target without stopping the session. The dismiss target
  is local overlay chrome, not a model tool or phrase-gated action.
- A capture or tool action must never blink the orb. Window-scoped capture does
  not mutate it. An action aimed through its footprint moves it clear instead of
  fading it. A receipt that both reports uncertain effect and carries a fresh
  reconciled observation is reduced atomically and never exposes an intermediate
  warning state.

### Foreground execution

- A listening/working Phone Control session runs as the correctly typed visible
  foreground service with an ongoing notification and the user-visible
  orb/overlay in both distributions.
- Start the session from a user-visible action. Do not depend on hidden
  background activity launches or an immortal background daemon.
- Microphone, media-projection, and playback service types follow the platform
  contract already declared by the mobile app. Process death retires owned jobs
  and requires state reconciliation; it never replays a command.
- Screen-off, lock, revoked overlay, and background-start denial are typed
  lifecycle/capability states, not reasons to fabricate an idle success.

Re-check Android's
[foreground-service changes](https://developer.android.com/develop/background-work/services/fgs/changes?hl=en)
and current [Android 17 behavior changes](https://developer.android.com/about/versions/17/behavior-changes-17)
when implementing the service boundary.

## Stable Tool And Capability Contract

Phone Control ports the Windows tool families instead of inventing an Android
prompt. Exact declarations remain owned by the canonical catalog builder.
Before implementation, either extract the provider-neutral declarations into a
shared generated schema or generate an Android artifact from the Windows-owned
schema. Do not maintain a hand-copied Kotlin catalog. Runtime MCP/integration
declarations append through the same versioned catalog boundary.

| Canonical family | Android implementation target |
| --- | --- |
| `observe`, `act`, `do_steps`, `click_at`, `look`, `click_target`, marks, zoom/view tools | Accessibility windows/nodes first; current screenshot, local detector, then vision when semantics are absent |
| `type_text`, keyboard, scroll, drag, pointer tools | Node actions and Accessibility input connection first; gesture dispatch or a proven elevated input backend as fallback |
| `open_url`, `launch_app`, window/app focus/list tools | Intents, package/task/display state, Accessibility global actions, and elevated system APIs when required |
| `system_query`, files, clipboard, and `run_command` | App APIs and persisted SAF grants first; Shizuku/root only for authority the app UID lacks |
| artifacts, memory, paste/extract/save tools | The shared artifact/memory schema with Android content URIs and persisted grants |
| browser read/navigation/tabs/eval/network/console/upload tools | Direct integration when available; credentialed browser CDP for full page authority; Custom Tabs for shared browser sessions; Accessibility for browser chrome/fallback; owned WebView only for SGT-owned or deliberately isolated content |
| research and web search | Shared research/source/evidence contract; research-owned tabs are turn-scoped and silently cleaned |
| app integrations and MCP | Shared declaration, schema, lifecycle, and typed failure contracts; platform transport is thin glue |
| `done` | The canonical terminal turn signal |

### Durable conversation memory

Phone Control memory uses an app-private store independent from the transient
artifact cache. Versioned session sidecars are the source of truth; a versioned
index is derived and rebuildable. Writes use same-directory durable temporary
files plus atomic replace. Startup promotes a complete newer temporary record,
discards a partial temporary record, isolates a corrupt sidecar without hiding
healthy sessions, rebuilds the index, and reapplies retention.

The turn assembler supplies explicit session, turn, record, and `USER` or
`ASSISTANT` role identity, with exactly one record for each role in a committed
turn. One atomic sidecar replacement appends the complete pair; an active
partial turn never reaches durable search state. A late finalized ASR revision
may atomically replace only the existing USER text in that same still-draft
pair; it cannot create a turn, change the assistant record, or reopen a finalized
session. Storage never infers roles from text. Draft sessions survive process death but remain absent from list, get, and
search-ready results until finalized. At process start, recovery drops any
incomplete tail, finalizes the remaining complete pairs, and deletes empty
drafts. Keep the newest 20 finalized sessions, preserve Unicode, and store no
screenshots. `search_memory` sees finalized sessions only and uses Unicode NFKC
phrase/term relevance with recency only as a tie-breaker; an empty query lists
the newest sessions. `open_memory` accepts only an exact returned session ID and
formats the full structurally labeled transcript. A future embedding provider
may improve ranking, but lexical retrieval remains the offline fallback and is
not part of durable storage. Play and full use this same implementation. The machine-readable contract is
`parity-fixtures/phone-control/memory-contract.json`.

Android `list_windows` means **current interactive surfaces**, not installed
apps, background tasks, or a history. API 30+ observations include Accessibility
windows on every display; API 29 is explicitly default-display only. Record a
window before attempting node traversal so a visible secure, blind, or otherwise
rootless surface still appears with `content_accessible=false`.

Each listed surface receives an observation-bound target of the form
`@android-window:v1:<snapshot_generation>:<display_id>:<window_id>:<package>`.
"Stable" means exact within that observation, not durable across observations.
A token from a retired generation returns `stale_target` with proven no effect
and requires a fresh `list_windows`. Exact package/title input is accepted only
when it resolves to one current surface; zero matches return `target_not_found`
and multiple matches return `ambiguous_target` with fresh token choices. Never
pick the first title/package match.

The same recovery rule applies to stale semantic `@id` actions, surface-token
actions, and batches. A proven-no-effect stale receipt must perform one fresh
Accessibility observation and attach its current generation, actionable
elements, and foreground surface targets to that same receipt. It never silently
rebinds or dispatches the old target. The attached observation is reconciled
state, so the model can make at most one retry using only identities from that
generation instead of looping on an expired snapshot.

Background visual streaming must not replace the semantic leases backing model
`@id` actions. Window topology and actual user-mutation events invalidate the
generation immediately. Generic window-content notifications advance a separate
visual revision instead of retiring every semantic lease; every semantic
mutation still resolves the live node path and exact fingerprint immediately
before dispatch. Coordinate actions require the visual revision captured with
their grid or detector verification, so content churn cannot turn an old image
into a click. Streaming may return the bitmap captured at one instant while
content continues changing. A topology or explicit mutation event during image
capture returns `stale_frame`, with only a bounded internal capture retry.

`focus_window` resolves one current token or one exact current package/title,
launches only the resolved launchable package, then takes a fresh observation.
Success requires that observation to prove the requested package is both active
and focused. An already-focused match is verified no effect. Dispatch without
that postcondition is `effect_may_have_occurred`, never success. A surface with
no launchable package returns `unsupported_on_surface`.

Accessibility mutation rejection before platform acceptance is proven no effect.
Once Android accepts a node action, global action, text edit, key sequence, or
gesture, its exact job retains the session effect slot through the provider's
terminal boundary. Gesture ownership ends only at `GestureResultCallback`;
synchronous Accessibility calls retain ownership through their bounded settle
and postcondition window. Coroutine cancellation cannot close either boundary
early. Accepted effects invalidate the leased snapshot and require
reconciliation because partial input may already have occurred.

Android does not expose arbitrary HWND-style geometry. `minimize_window` has one
narrow honest route: when the target is the sole active fullscreen app, perform
Home and verify from a fresh observation that the target is no longer
foreground. Split-screen, picture-in-picture, system, overlay, and ambiguous
surfaces return `unsupported_on_surface`. `move_window` and `resize_window`
always return `unsupported_on_surface` for arbitrary Android surfaces. Keep all
three tools declared; never fake success or hide them.

Android visual observation uses the Accessibility screenshot provider on the
default display. On API 34+, active-surface frames use the exact Accessibility
window id with `takeScreenshotOfWindow`, so controller overlays are excluded
without hiding, fading, detaching, or otherwise mutating visible UI. Whole-display
capture and the API 30-33 compatibility path use a bounded display-capture
suppression scope because Android exposes no older window-scoped screenshot API. Normal
frames carry the same numbered 6x5 grid geometry used by `click_at`, `drag`, and
`zoom`. Every grid and crop is bound to the observation generation, display,
window, package/surface, rotation, density, capture timestamp, absolute screen
crop, and visual-content revision. `zoom` accepts only a cell from the current frame and magnifies that
cell with one-quarter-cell context; a changed generation returns `stale_frame`
with proven no effect. `reset_view` captures the fresh active application
surface. `see_whole_screen` captures the complete default display and reports
that display scope rather than implying unavailable multi-display pixels.

An API 34+ window id may expire after observation but before the platform
accepts its screenshot request. That typed invalid-window result is a capture
race, not a broken screen provider: the same capture operation retries once
through the display-scoped Accessibility route, preserving the requested
absolute crop and suppressing only the controller overlay. Any retryable frame
failure receives a bounded two-attempt grace period; the third consecutive
failure becomes visible degradation, while one successful transmitted frame
clears the failure state. Non-retryable failures remain visible immediately.
This recovery depends only on typed platform outcomes and never on the current
app, surface text, coordinates, or user language.

`look` does not run a second language agent or synthesize a reading in code. It
places one clean, ungridded capture of the current view into the same live
model's input before the tool receipt and reports exact frame metadata; the live
model owns the requested visual meaning. Secure capture, display mismatch,
provider loss, and screenshot rate limits remain typed failures. `point_at`
stays unavailable because Android has no universal persistent touch pointer or
hover state. `drag_target` selects both described endpoints from one immutable
marked frame, then rebinds both through one fresh detector inference and exact
surface lease. It verifies a fresh crosshair crop for each endpoint and performs
one Accessibility swipe only after a final exact-surface observation.

### Capability registry

Each provider probe records:

- provider ID and authority identity (`app`, `accessibility`, `shell`, `root`,
  `device_owner`, or `privileged_system`);
- state and evidence timestamp;
- Android/API/OEM scope and display/user/profile scope;
- required grant, service, pairing, or user action;
- whether the provider survives process death and device reboot;
- supported capability IDs and known structural limits.

Provider choice uses the narrowest ready provider that supplies the full
requested semantics. Stronger authority is not automatically better evidence.
For example, a fresh Accessibility node action beats a shell-coordinate tap,
and a DOM node beats either for a browser element.

### Baseline Accessibility backend

The existing service already retrieves interactive windows and can capture the
display. Phone Control additionally requires:

- general immutable window/node snapshots rather than text-selection-only
  traversal; API 30+ enumerates all displays, API 29 declares its default-display
  limit, and a window snapshot survives absent/inaccessible root content with
  `content_accessible=false`;
- session-scoped window/content/scroll event subscriptions sufficient to
  invalidate snapshots, narrowed again while idle; events are hints, never a
  substitute for a fresh observation;
- `android:canPerformGestures="true"` before using `dispatchGesture()`;
- global actions such as Back, Home, Recents, and notifications where supported;
- node click, focus, set-text, selection, scroll, expand/collapse, and other
  advertised `AccessibilityAction`s;
- API 33 Accessibility `InputMethod` support with
  `FLAG_INPUT_METHOD_EDITOR` for robust multilingual text, cursor, selection,
  surrounding-text, and key-event behavior;
- Android `type_text` and `key_combination` first validate the current surface
  target emitted by `list_windows`, then resolve the one focused editable node
  inside that exact live window. A node `@id` is snapshot-local implementation
  detail and cannot substitute for the surface target. API 33 input-connection
  insertion is preferred; the older `ACTION_SET_TEXT` route runs only when the
  full current text and selection are observable, so append/selection replacement
  is exact. Exact single-key Android system navigation (`back`, `home`,
  `recents`, `notifications`, and `quick_settings`) uses the same
  `key_combination` surface token but dispatches a structurally leased
  Accessibility global action without requiring a focused editor or pointer
  geometry. The exact current non-controller platform-window lease—including
  Android system surfaces such as notification shade—is revalidated at
  dispatch. If only the content generation retired, system navigation may
  continue from the old token only when its display, window, and package still
  identify that one current foreground surface; dispatch then uses a newly
  captured surface lease. This narrow continuation never rebinds an element or
  pointer geometry. Inactive higher windows cannot intercept a global action,
  while an active Android-owned user step still blocks it. It works in Standard mode;
  Shizuku/root may only provide an honest same-semantics upgrade.
  Other keys remain editor-bound. Desktop-only chords fail typed-unsupported.
  `paste_artifact` has no
  target parameter in the canonical catalog: it resolves UTF-8 text locally,
  takes a fresh observation, and proceeds only when there is one unique focused
  editable node on the active surface. It never sends the artifact body through
  the model;
- API 30 display screenshots and API 34 per-window screenshots when overlays
  would contaminate the target;
- explicit rate-limit, invalid-window, no-access, and secure-window failures.

Android documents gesture capability on
[AccessibilityService](https://developer.android.com/reference/android/accessibilityservice/AccessibilityService),
the API 33 editor on
[InputMethod](https://developer.android.com/reference/android/accessibilityservice/InputMethod),
and screenshot limits in the same AccessibilityService reference.

Do not retain `AccessibilityNodeInfo` objects as durable identities. A target is
valid only inside one observation snapshot and includes at least snapshot
generation, display ID, accessibility window ID, package, node path/index,
bounds, and surface/document identity. Any mutation, navigation, rotation,
window change, or uncertain interruption invalidates it.

### Visual grounding and the local detector

- Semantic Accessibility/DOM state remains first choice.
- A screenshot is bound to its exact display/window, crop, rotation, density,
  insets, snapshot generation, and capture timestamp.
- UI-DETR-1 keeps its Windows role: optional class-agnostic clickable-region
  marks only on semantically blind surfaces. It does not infer user intent and
  never authorizes a click by itself.
- Detector boxes become numbered current-frame anchors. The model still chooses
  the target; execution performs fresh crop verification and postcondition
  checks.
- The Android port may use the same validated model only after measuring APK/
  download size, memory, latency, thermal cost, and mark accuracy on real
  devices. It stays an on-demand asset delivered equivalently through the
  distribution's supported module/asset mechanism.
- ONNX Runtime recommends starting with CPU/XNNPACK for unquantized mobile
  models and measuring NNAPI because partitioning and benefit are model/device
  dependent. Run the model usability checker, then benchmark CPU, XNNPACK, and
  NNAPI; do not select an execution provider from device branding.
- The Android implementation uses the verified Windows tensor contract
  (`input` 1x3x1024x1024, ImageNet normalization, named `dets`/`labels`
  outputs, 0.70 score threshold, 0.92 duplicate IoU, 90 retained proposals and
  30 displayed marks). CPU is the conservative baseline until the required
  real-device provider benchmark selects anything else.
- Both distributions bundle the small `libonnxruntime4j_jni.so` Java bridge.
  The full distribution packages the checked-in `ort-runtime.zip` as a
  Full-only asset; Play delivers the same core runtime through
  `feature_asr_ort`. Full downloads the UI-DETR model through its existing
  verified on-demand path. Play packages the exact same 131,216,489-byte model
  in that on-demand feature and copies it into the shared stable model path only
  after split activation. Both routes require the canonical SHA-256 before use;
  Play never falls back to a second network model download.
- Both flavors load `libonnxruntime_real.so` before native/JNI consumers. The
  packaged `libonnxruntime.so` compatibility proxy exposes only the API-table
  entry point and must never substitute for the real runtime's complete ABI.
- `parity-fixtures/phone-control/native-runtime-contract.json` is the sole
  identity owner for every checked-in native-runtime archive and member. Builds,
  Full extraction, and Play compliance require exact byte counts and SHA-256
  digests. Runtime ZIPs contain only unique, flat, declared filenames; extraction
  stages and verifies every member before atomic same-directory finalization.
  ORT never depends on a mutable network branch. The remaining Full runtime
  downloads use the same exact archive/member contract and fail closed if their
  remote bytes change.
- Because UI-DETR is class-agnostic, `map_targets` publishes numbered anchors
  on the exact captured frame and `click_mark` re-runs the detector and requires
  at least 0.35 box IoU before input. Android `click_target` keeps the canonical
  single-tool behavior: an auxiliary vision model chooses one current numbered
  anchor from the requested description, then the provider re-runs UI-DETR,
  verifies that anchor against the exact frame/surface lease, and asks the
  vision model to confirm that a crosshair on the fresh crop is inside the
  requested target with at least 70% confidence. The gesture dispatches only
  after both checks. Language meaning never moves into Kotlin and no second
  tool call is required. `drag_target` applies the same division of labor to two
  endpoints: auxiliary vision chooses two distinct current anchors in one call;
  one screenshot and one UI-DETR inference independently rebind both anchors;
  both fresh crops pass the same crosshair threshold; and one leased swipe is
  dispatched. Sequential mixed-frame endpoint refreshes and zero-distance
  fallback drags fail closed.

See ONNX Runtime's [mobile deployment guidance](https://onnxruntime.ai/docs/tutorials/mobile/),
[XNNPACK provider](https://onnxruntime.ai/docs/execution-providers/Xnnpack-ExecutionProvider.html),
and [model usability checker](https://onnxruntime.ai/docs/tutorials/mobile/helpers/model-usability-checker.html).

## Optional Authority Providers

### Ordinary Android and special-access APIs

Use normal APIs before elevated bridges: intents, ContentResolver, MediaStore,
persisted Storage Access Framework grants, MediaSession, Notification listener,
runtime permissions, overlay access, and other user-granted special access.
Persisted URI grants are first-class resources, not filesystem-path guesses.

Filesystem mutations sharing one canonical path are serialized within SGT.
`save_artifact` with `overwrite:false` uses an atomic create-only operation, and
an exact text replacement revalidates its expected hash immediately beside the
atomic replacement. A concurrent creator or modifier is reported without
silently overwriting its bytes.

Prefer a precise app-owned integration over UI automation when it exposes the
requested semantics without reducing scope. This includes ordinary Android
APIs, current MCP/integration providers, and Android AppFunctions if SGT becomes
an authorized caller. AppFunctions is an Android 16+ experimental preview, not
a baseline dependency or a reason to hide the general UI tools. It enters the
same capability registry and returns typed `unsupported` or
`needs_user_step` states when unavailable. See the current
[AppFunctions overview](https://developer.android.com/ai/appfunctions).

The Storage Access Framework requires a system picker and cannot grant every
protected directory. See
[shared document access](https://developer.android.com/training/data-storage/shared/documents-files).

### Shizuku shell backend

- Integrate through the Shizuku API and request its permission as a real runtime
  grant. Verify the service UID/authority; ADB-started and root-started services
  do not have identical power.
- The setup wizard may open Developer Options/Wireless debugging, observe the
  current surface, and guide pairing. The user still performs Android-owned
  pairing, trust, and confirmation steps.
- Selecting Shizuku is event/return driven and bounded by structural probe state.
  SGT gives short localized feedback before leaving its surface, opens only the
  official manager/store/download route for the observed state, re-probes on
  return, and automatically requests SGT's Shizuku permission once the Binder is
  ready. It never branches on localized Shizuku labels, clicks private manager UI,
  captures a pairing code, repeats an unchanged external step, or claims the
  provider is ready before the grant probe succeeds.
- On current non-root Android, Shizuku's wireless-debugging service must be
  started again after reboot. Record `needs_user_step` after a failed boot probe.
- Elevated commands use the same exact operation ID in the app process and the
  Shizuku user service. The AIDL bridge cancels that command only; it never
  destroys the shared user service or another command. Local root and Shizuku
  workers retain terminal ownership until the root process and owned descendant
  tree are confirmed dead where Android exposes process-tree handles. A blocked
  or failed Binder cancellation attempt cannot block the lifecycle caller and
  cannot be dropped: it remains owned settlement work until that attempt exits.
- Never translate a shell failure into success. SELinux, OEM changes, user/profile
  boundaries, and shell permission limits remain real.

See the [Shizuku API](https://github.com/RikkaApps/Shizuku-API) and
[current setup/reboot behavior](https://shizuku.rikka.app/guide/setup/).

### First-party SGT privileged bridge research track

A future local ADB bridge may remove the external Shizuku-app dependency, but
only after a threat model and real-device prototype prove pairing, discovery,
reboot, and Chrome socket access. Its contract is:

- bind only locally; never expose an unauthenticated LAN command server;
- keep ADB keys in Android Keystore and provide explicit revoke/forget controls;
- show the Android pairing/trust UI and never capture or bypass an OS-owned secret;
- authenticate the app/bridge endpoint, scope each job, and return effect receipts;
- survive process interruption without replaying commands;
- require Android 17 `ACCESS_LOCAL_NETWORK` when applicable.

Android 17 plus adb 37 can automatically reconnect a paired device to a trusted
workstation network. That does **not** prove that an on-device bridge or Shizuku
service auto-starts after reboot. Keep this as an experimental result until a
device test proves it. See [ADB Wi-Fi 2.0](https://developer.android.com/studio/run/device)
and the [Android 17 local-network requirement](https://developer.android.com/about/versions/17/behavior-changes-17).

### Root, device owner, and privileged-system backends

- Root/Sui is optional. Request root through the installed root manager, verify
  the resulting UID, constrain the local bridge, and retain the same effect
  checkpoints and receipts.
- Device owner is an enterprise/provisioning mode, not a normal permission. It
  may expose device-policy actions only on correctly provisioned devices.
- A platform-signed/privileged-system build is a separate deployment target for
  controlled devices. Never imply that the normal APK can acquire it.
- These providers add authority; they do not replace Accessibility/DOM evidence
  or weaken the Windows consequential-effect boundary.

See Android's [DevicePolicyManager](https://developer.android.com/reference/android/app/admin/DevicePolicyManager).

## Browser Contract

- Credential continuity and control authority are separate dimensions. A
  surface may have the user's signed-in browser session without exposing a DOM,
  and a provider may control a page without owning browser chrome or OS prompts.
  Provider selection records both dimensions instead of treating "in a browser"
  as one capability.
- Android Chrome does not support the desktop extension path. Phone Control must
  work without a browser extension.
- Prefer the following surface-aware ladder while keeping the same stable tool
  catalog:
  1. a precise direct app integration when one supplies the requested semantics;
  2. CDP attached to an existing credentialed Chrome/Chromium page target;
  3. a Custom Tab for navigation that needs the user's preferred-browser session,
     followed by CDP only if a current probe discovers and binds its exact target;
  4. Accessibility for browser chrome, other browsers, OS/login surfaces, and
     semantic fallback, then current-frame detector/vision grounding;
  5. an SGT-owned WebView only for SGT-owned content or an intentionally isolated
     app-private session.
- Custom Tabs are powered by the user's preferred browser and normally share its
  cookies, permissions, saved credentials, and other browser state. That gives
  credential continuity, not generic DOM, network, console, upload, or JavaScript
  authority. Custom Tabs APIs and `postMessage` are used only for their documented
  lifecycle/UI contract or a cooperating verified origin; they never masquerade
  as a general page-control bridge.
- A normal owned WebView has an app-private cookie/session store and does not
  inherit the user's browser login. Never copy or export browser cookies,
  passwords, tokens, or credential databases into SGT to bridge that gap. Some
  identity providers also reject embedded user-agents under developer control,
  so an owned WebView cannot be the universal authenticated-browser route.
- A Chrome/Chromium CDP provider may expose DOM, tabs, navigation, page reads,
  console, network, upload, evaluation, page screenshots, and trusted page input
  after a real-device bridge proves the `chrome_devtools_remote` route and exact
  target ownership. CDP owns page targets only; it does not control Android
  browser chrome, permission sheets, account choosers outside the page target,
  or other OS-owned UI.
- CDP transport must stay device-local and authenticated through a proven
  Shizuku, root, or first-party bridge. USB/wireless-debugging trust remains an
  Android-owned user step. Never expose a remote-debugging endpoint on the LAN,
  reuse another app's pairing material, or treat a reachable socket as ownership
  of every tab.
- Opening a Custom Tab and discovering that same surface as a CDP target are two
  separately evidenced transitions. Target discoverability must be probed across
  supported browser/version combinations. If the target is absent, keep the
  authenticated Custom Tab and route only semantics that Accessibility can
  honestly preserve; CDP-only tools return `capability_unavailable`.
- SGT-owned WebViews use a direct, authenticated JS/native bridge and stable web
  surface identity. Third-party WebViews are CDP-visible only when their owning
  app enables WebView debugging. Otherwise use Accessibility and vision and
  report CDP-only tools as unavailable.
- Browser targets include browser package/profile scope, credential-context kind
  without secret material, tab/target ID, document ID, loader/navigation
  generation, frame/surface, and observation generation. A Custom Tab launch is
  not proof of a CDP target, and a visible URL/title match alone is not ownership.
- Research-owned and disposable tabs follow the Windows turn-lifetime contract.
- Browser content and auth state stay inside the provider. Logs/traces may record
  the credential-context kind (`attached_browser_tab`,
  `custom_tab_shared_state`, or `app_private_webview`) but never cookies, tokens,
  passwords, autofill values, pairing secrets, or credential-store paths.

Chrome documents Android remote debugging and the local abstract socket in
[remote debugging](https://developer.chrome.com/docs/devtools/remote-debugging/),
while [WebView debugging](https://developer.chrome.com/docs/devtools/remote-debugging/webviews)
must be enabled by the owning app. Chrome's
[Custom Tabs overview](https://developer.chrome.com/docs/android/custom-tabs/)
documents browser-state sharing and the WebView separation. Google OAuth
[requires secure browsers](https://developers.google.com/identity/protocols/oauth2/policies#secure-browsers)
rather than developer-controlled embedded user-agents. Mobile Chrome extensions
are not a supported dependency; [Chrome's extension help](https://support.google.com/chrome_webstore/answer/1698338?hl=en)
limits them to computers.

## Consequential Effects

- Requested routine reversible actions proceed.
- Confirm only an unrequested irreversible, destructive, financial,
  privacy-sensitive, or external-commitment effect, at the effect boundary.
- Text entry never implies submit, send, publish, buy, install, grant, delete, or
  confirm.
- Every observed target carries immutable effect authority derived only from
  platform structure: `routine`, `consequential`, or `os_owned_user_step`.
  Localized labels, model prose, and user phrases never assign or clear it.
- Being preinstalled or system-signed does not make an app an OS-owned user
  step. Only a capability-derived platform authority on the matching live
  surface, or an active opaque platform user-step session, may assign that state.
- Consequential authority likewise needs platform effect metadata. Android's
  explicit Accessibility dismiss action is consequential; a generic clickable
  node is not promoted from its label, app identity, or visual appearance.
- The authority check is a provider-side dispatch invariant, not a semantic-tool
  convention. Semantic nodes, coordinate clicks, detector marks, long presses,
  drags, scrolls, text edits, and key sequences must all present an immutable
  observation-bound node or surface lease before Android receives input.
- Elevated command execution must perform the same fresh structural preflight
  immediately before process dispatch. While an active Android-owned user-step
  window is present, no shell or root command is dispatched; command text is
  never parsed to guess intent or create exceptions.
- Platform APIs that report a pending confirmation register an opaque user-step
  session before presenting their system-owned UI and retire it on resolution,
  failure, or cancellation. Authority checks consume only that structural
  session state and live window identity, never prompt text or user wording.
- A mutation lease binds snapshot generation, display, window, package/surface,
  layer, bounds, and authority. The provider rejects a stale or mismatched lease,
  an unknown authority identity, or a higher interception surface before dispatch;
  another tool route cannot weaken the target's authority.
- `confirm:true` can cross only a structurally consequential app-effect
  checkpoint. It never automates an OS-owned confirmation; that receipt remains
  proven-no-effect and names the required user step.
- Enabling Accessibility, restricted settings, wireless debugging, root, device
  owner, VPN, notification access, media projection, and similar OS authority is
  a setup action with its own system UI. User words do not bypass that UI.
- Lock-screen credentials, biometrics, passkeys, payment confirmation, and other
  OS/hardware-owned authentication always remain a user step.
- An interrupted elevated command is proven no effect only before process
  acceptance. After process start it remains may-have-occurred until exact
  process-tree termination is acknowledged; an uninstrumented mutator stays
  unknown. It follows the same reconciliation rule as other accepted mutations.

## Failure And Recovery

Every tool result includes enough structure for the model and trace oracle to
distinguish no effect, verified effect, and unknown effect. At minimum:

- stable `code`, capability ID, requested tool, provider, and provider state;
- turn/job identity and the observation generation used by the action;
- `effect_may_have_occurred` and `effect_verified`;
- snapshot invalidation and fresh-observation requirement;
- retryability and bounded retry class;
- missing grant/backend and a machine-readable `required_user_step` when one
  exists;
- current display/user/profile/surface scope when relevant.

Hard walls return typed failures: secure/DRM capture, stale nodes, inaccessible
profiles, OEM-omitted nodes, unsupported multi-window effects, revoked services,
shell/SELinux denial, unavailable WebView debugging, missing CDP, lock screen,
and OS-owned confirmation. Do not narrate success, loop setup, or downgrade a
different tool into an apparent success.

Provider death or revocation invalidates its targets immediately. Re-probe,
reconcile uncertain effects, and continue through another provider only when it
preserves the requested semantics and reports the route change.

The activation capability snapshot is evidence for the model and activation
coordinator, not an authority gate in front of implemented composite handlers. Provider readiness
can change between snapshot and dispatch; the provider-specific handler owns the
fresh probe and the dispatch boundary validates its exact receipt against the
tool plan and capability route.

### Diagnostic evidence

- Phone Control writes a bounded two-file JSONL diagnostic journal under its
  app-specific external-files directory. Writes are asynchronous and best
  effort: diagnostic failure or backpressure can never affect the runtime.
  `mobile/scripts/collect-phone-control-diagnostics.ps1` collects that journal
  plus filtered Logcat from one exact device, Android user 0, and package.
- The persistent journal accepts structural event summaries only. It preserves
  Unicode but never persists exception messages or stack traces. Call sites
  must not place speech, model text, node text, URLs, paths, clipboard/file/page
  content, command output, keys, tokens, or authentication material in an event.
- Structured traces carry turn, generation, job, snapshot, surface, capability,
  provider, timestamps, cancellation, receipt, postcondition, and typed-error
  identity. This is the diagnosis source; console prose is only a safe summary.
- Preserve Unicode in trace artifacts. Never log encoded/garbled substitutes
  when the original provider text is available.
- Transcripts, node text, screenshots, clipboard data, file contents, browser
  content, and command output keep explicit privacy classes and are captured
  only under the trace/evidence policy.
- An Accessibility node structurally marked `isPassword` is represented as
  protected content. Its text/value and every hash, preview, fingerprint, log,
  artifact, or model field derived from any text-like node field are omitted.
  Content description, hint, and state description are also dropped because an
  app can copy secret material into them. Only structural role, view ID, bounds,
  actions, and an explicit protected marker remain model-visible.
- Accessibility-backed `browser_extract_page` returns artifact identity and
  capture counts without inline page or artifact preview content. The stored
  artifact is built only from the same protected-field-safe capture.
- Ordinary Logcat may report clipboard item count or text presence, never a
  content preview.
- Secrets, pairing codes, authentication material, and unredacted protected
  fields never enter ordinary logs or benchmark artifacts.
- The independent oracle consumes state/effect evidence, not the model's claim
  or a final caption.

## Fixtures

- Shared authority/routing fixture:
  `parity-fixtures/phone-control/authority-matrix.json`
- Shared turn/effect fixture:
  `parity-fixtures/phone-control/turn-contract.json`
- Shared file-mutation fixture:
  `parity-fixtures/phone-control/file-mutation-contract.json`
- Shared native-runtime identity fixture:
  `parity-fixtures/phone-control/native-runtime-contract.json`
- Shared launcher/activation fixture:
  `parity-fixtures/phone-control/activation-flow.json`
- Shared socket lifecycle fixture:
  `parity-fixtures/gemini-live-session/lifecycle.json`
- Windows acceptance suite:
  `tests/computer_control_golden_suite.json`
- Windows visual benchmark contract:
  `tests/VISION_GROUNDING_BENCHMARK.md`

### Verified emulator evidence (2026-07-18)

- Full and Play unit suites pass with 454 and 443 tests respectively; neither
  suite has failures, errors, or skips.
- Clean Full and Play installs each pass five instrumentation tests on
  `emulator-5554`. Both load the same exact ORT libraries and 131,216,489-byte
  UI-DETR model, then run CPU inference on the current device frame. Full proves
  its bundled archive path; Play proves initially absent on-demand splits.
- The final Play release AAB passes module ownership plus exact native/model
  byte-count and SHA-256 checks.
- Production-path probes on real Settings surfaces verify a routine navigation
  postcondition, stale-target rejection with proven no effect, and an OS-owned
  Package Installer confirmation that remains a user step and preserves the
  installed package.
- The emulator's AOSP Camera entered its own ANR dialog before a usable blind
  preview existed. Phone Control returned a typed degraded, proven-no-effect
  result. A successful blind-surface detector action and optional CDP, Shizuku,
  root, and OEM/device variants remain real-device evidence gaps, not claimed
  passes.

Platform tests must consume the shared fixtures rather than duplicate their
constants. Required Android coverage:

- physical-device harnesses bind every ADB call to an exact serial, reject
  pre-existing debug/test packages before destructive clean-install work, journal
  recoverable device state durably, remove only run-owned packages, restore and
  verify harness-owned Accessibility/overlay state, best-effort restore the
  foreground displaced by the run, and prove the normal release package was
  untouched;
- debug probe dispatch derives mutation classification from the production tool
  registry and requires an explicit host acknowledgment; it never maintains a
  second tool-name allowlist;

- catalog stability across every capability-state combination;
- setup return/probe/revocation/reboot transitions;
- Accessibility snapshot identity, stale rejection, gestures, text editing,
  screenshots, rotation, insets, multi-window, multiple displays, and secure
  capture failures;
- one final response, settled idle, silent cleanup, current-generation speech,
  barge-in, reconnect, late-event retirement, and unknown-effect reconciliation;
- ordinary API, SAF, Shizuku-shell, root, and device-owner route selection on
  devices where each backend exists;
- an already signed-in normal Chrome tab over CDP, including page-versus-browser-
  chrome boundaries and CDP disconnect/revocation;
- standard non-ephemeral Custom Tab browser-state continuity, target discovery
  across browser/version combinations, exact target binding when discoverable,
  and Accessibility fallback when it is not;
- owned-WebView isolation from the preferred-browser session, embedded-login
  rejection/redirect behavior, third-party debug-enabled WebView, a non-Chrome
  default browser, Accessibility-only browser control, and visually blind
  surfaces;
- proof that no browser secret enters SGT storage, traces, screenshots intended
  for benchmarks, or provider handoff payloads;
- UI-DETR CPU/XNNPACK/NNAPI latency, memory, power, and strict-box accuracy before
  choosing a default execution provider;
- a clean Play local-testing AAB install that proves the ORT and shared-C++
  splits were initially absent, delivers them on demand, validates the bundled
  UI-DETR bytes, loads ORT, and runs inference on the current device frame. A
  user-scoped installer must preserve bundletool's local-testing contract by
  staging every non-base-master split in the manifest-declared directory and
  making that directory app-readable before the first split request;
- API 29 baseline plus representative API 30, 33, 34, and current devices across
  multiple OEMs, densities, navigation modes, orientations, and accessibility
  configurations.

## Real-Task Evaluation

Port the Windows golden-suite invariants, not its desktop applications. Use
natural goals, disposable data/accounts, independent oracles, and one correction
or disruption after meaningful progress. Opening a settings, landing, or pricing
page is never useful completion by itself.

Use a small, high-yield rotating task set informed by:

- [AndroidWorld](https://google-research.github.io/android_world/) for dynamic
  tasks with programmatic reward/oracles;
- [Android in the Wild](https://arxiv.org/abs/2307.10088) for visual and gesture
  diversity;
- [MobileWorld](https://arxiv.org/abs/2512.19432) for vague, long-horizon,
  cross-tool tasks;
- [B-MoCA](https://proceedings.mlr.press/v330/lee26a.html) for device/configuration
  diversity;
- [MobileAgentBench](https://mobileagentbench.github.io/) for broad capability
  coverage.

Do not copy whole benchmark suites or optimize production logic for their apps,
phrases, layouts, or expected routes. Select few tasks that jointly expose
planning, semantic/visual grounding, long/short horizons, files, browser,
notifications, interruptions, elevated authority, and consequential boundaries.

Security cases run only in disposable emulators/devices or restorable profiles
with fake accounts, canary secrets, restricted egress, and mock endpoints. Stop
on canary disclosure, unexpected egress, destructive effects, or missing
consequential checkpoints. One initial run and at most one repair rerun per case.

## Implementation Order

1. Shared/generated tool schema, Android capability registry, fixtures, trace
   schema, and a shared Play/Full module boundary.
2. Agent lifecycle/audio ownership plus the stable full catalog with typed
   unavailable results.
3. General Accessibility snapshots/actions/gestures/input/screenshots and
   postcondition receipts.
4. Android app/system/file/artifact/memory/research/MCP adapters.
5. Real-device credentialed Chrome CDP prototype, then Custom Tab launch/target
   discovery and the Accessibility browser-chrome/fallback matrix. Keep the
   owned-WebView bridge for SGT-owned or deliberately isolated content.
6. Optional Shizuku shell backend and no-brainer setup/reboot diagnosis.
7. First-party local ADB bridge prototype; promote it only after it matches the
   Shizuku route's authority, lifecycle, security, and real-device reliability.
8. Optional root/device-owner/privileged providers behind the same registry.
9. UI-DETR mobile feasibility benchmark and integration only if it improves the
   real blind-surface route.
10. Real-task golden runs, security isolation, performance/thermal testing, and
   one repair rerun per failed acceptance task.

Do not start with broad UI polish, app-specific scripts, or a duplicated Android
prompt. The first usable slice must already obey catalog, target identity,
effect receipt, terminal completion, audio ownership, and typed failure rules.

## Deviations

- Product label differs: Windows **Computer Control**, Android **Phone Control**.
- Android capability acquisition is grant/provider based rather than Windows
  integrity-level based.
- Desktop HWND geometry has no universal Android equivalent; unsupported
  surfaces fail explicitly.
- Android browser control has no extension dependency. Direct integrations,
  credentialed CDP, Custom Tabs, Accessibility, and current-frame grounding form
  a surface-aware ladder; owned WebViews remain isolated unless the user signs
  into that separate store.
- MediaProjection consent is session-scoped on modern Android and cannot be
  cached as a perpetual grant. See Android's
  [media-projection guide](https://developer.android.com/media/grow/media-projection).
- Shizuku ADB startup currently does not survive reboot; root/device-owner/system
  deployments have different lifecycle contracts.
- All other behavior defaults to the Windows contract.
