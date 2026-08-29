# Phone Control Parity Contract

Status: core implementation complete; physical-device acceptance remains in
progress. Evidence is recorded only after each contract layer is exercised;
untested authority and device variants remain explicitly open.

Research baseline: 2026-07-18. Re-check the linked Android, Play, Shizuku, and
Chrome documentation before implementation because platform and store rules
change independently of SGT.

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
  - `src/overlay/computer_control/vision_contract.rs`
  - `src/overlay/computer_control/vision_reader.rs`
  - `src/overlay/computer_control/telemetry.rs`
- Canonical end-to-end evaluation:
  - `tests/COMPUTER_CONTROL_GOLDEN_SUITE.md`
  - `tests/computer_control_golden_suite.json`
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
- Full distribution downloads shared native runtime archives from the SGT
  runtime-bundles release and verifies their exact identity before installation.
  Play distribution receives the same payloads through Google-hosted on-demand
  feature modules. Lightweight application and mini-app frontend code remains
  embedded in both distributions.
- Store listing and policy compliance are release-review concerns, not runtime
  phrase gates or source-set feature removal. Keep policy declarations and user
  disclosures truthful without changing requested semantics.
- The goal is the strongest control Android permits after explicit user grants.
  It is not a promise to bypass the lock screen, secure surfaces, hardware-backed
  authentication, OS-owned confirmations, SELinux, or unavailable OEM APIs.
- One runtime contract covers Android devices and form factors. Product behavior
  may branch on probed capabilities, Android API contracts, grants, provider
  readiness, and live display/window geometry, density, insets, or rotation. It
  must never branch on manufacturer, brand, model, product, serial, emulator
  identity, a recorded resolution, or localized UI text.
- Accessibility is the baseline semantic/control backend. A whole-display
  MediaProjection session is required for every running Phone Control session;
  it supplies the display-wide pixel route while Accessibility supplies exact
  window pixels, semantics, and actions. Shizuku, root, device owner, direct app
  APIs, browser debugging, notification access, and future privileged
  deployments are optional capability providers, not alternate agent designs.
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
- A requested outcome authorizes inspection and use of the on-device state
  needed to complete that outcome, within the requested effect scope. Resolve
  operational details from current device evidence and tools before asking the
  user. A ready provider is execution authority, not a reason to downgrade the
  task to advice or delegate resolvable steps back to the user.
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
- Provider readiness is resolved at dispatch time. The setup-time capability
  summary is compact, never lists unselected optional providers as blockers,
  and cannot veto a declared tool after an authority changes. A selected ready
  shell authority is advertised as a direct `run_command` backend.
- Device-shell execution is independent of Accessibility observations and
  visual target leases. It owns only its exact command job, cancellation, and
  process receipt. Semantic element actions and coordinate actions continue to
  require their exact current target or frame leases.
- Unknown future integrations remain available by default through the same
  catalog and capability registry.
- One user turn produces at most one final response, then settles idle.

`parity-fixtures/phone-control/authority-matrix.json` is the machine-readable
authority and routing contract. Catalog availability is never inferred from
the user's words.

Pixel grounding uses the catalog-owned ordered model chain and fallback contract
in `parity-fixtures/phone-control/model-chain.json`. Windows Computer Control and
Android Phone Control consume the same generated IDs; neither platform owns a
second locator-model constant. The current chain uses Robotics ER2 as the
strict-accuracy primary and Gemini 3.5 Flash Lite as the faster fallback.
General screenshot reading remains separate from target grounding and uses the
effective shared Image-to-Text chain, including signed Live-feed interleaving.
The dedicated grounding chain never inherits those general candidates.

The live control session uses the fixture's exact Gemini Live endpoint and
bounded `LOW` thinking configuration on both platforms. Silent thought parts
feed control intent only and are never narrated or shown to the user.
In the session's `AUDIO` response mode, raw model text is also control-plane
material: only provider `outputTranscription` and PCM audio count as assistant
content. Raw text parts cannot update the orb, transcript memory, emotion
analysis, or turn-completion evidence.
Voice sessions also use the same high-sensitivity automatic activity detection,
30 ms prefix padding, 250 ms end silence, and native start-of-speech
interruption. Both platforms append the same compact evidence-routing and
communicative-intent invariants to the canonical prompt. Those rules rank
current semantic, pixel, browser, system, file, and integration evidence; they
never encode user phrases, languages, applications, or device identities.

External invocation follows
`parity-fixtures/phone-control/external-control.json`. Windows owns the canonical
`launch`, `submit_turn`, `status`, `cancel`, and `stop` lifecycle. Android maps
that lifecycle to its system assistant entrance: only while SGT holds the
default-assistant role may `ACTION_ASSIST` carry one bounded transient
`Intent.EXTRA_TEXT` goal. The exported gateway never persists the goal and
passes it only through the private activation coordinator into the ordinary
Phone Control runtime. A running idle session accepts it immediately; startup
or capture-resume holds at most one goal until the runtime can accept it. Blank,
oversized, unapproved, stale, or concurrent goals are rejected without changing
the session. The gateway requires Android's signature-level
`BIND_VOICE_INTERACTION` permission in addition to checking that SGT currently
holds the assistant role, preventing an ordinary application from explicitly
invoking the exported activity. Full and Play behavior is identical.

Phone Control UI strings have complete default-English, Korean, and Vietnamese
resource sets; Android's normal default-resource fallback serves every other UI
locale. The Apps card title resolves from the same in-app language catalog as
the rest of the carousel, rather than Compose's device-locale resource context.
OS-surfaced Phone Control text resolves through the in-app locale-aware Android
resource context. `parity-fixtures/phone-control/localization-contract.json`
records the supported titles, fallback, and resource parity invariants. The live
model is not pinned to the app locale or a language list. Input and output
transcription stay enabled and provider-detected, so speech meaning, planning,
and replies remain multilingual without language-specific routing.

## Behavior Contract

### User-visible flow

1. Phone Control lives directly on the Apps card in both APK distributions.
   The card has the same compact **Turn on / Turn off** interaction as Live
   Translate and sits directly beside it in the app carousel; it does not open a
   Phone Control page.
   Android also exposes one dedicated `ACTION_ASSIST` activity so the package is
   eligible for the system's default digital-assistant selection. The exported
   activity is a stateless platform gateway; the activation coordinator remains
   private. A system assistant invocation starts the same activation flow when
   Phone Control is stopped, requests capture resume when a live session is
   capture-suspended, and otherwise preserves the already-running session. It
   ignores every other action and every assist-context extra except the bounded
   `Intent.EXTRA_TEXT` goal defined by the external-control fixture.
   Each actionable invocation explicitly re-enters the app-owned coordinator
   task with new-task, clear-top, and single-top semantics, so an app Settings
   surface above the stateless gateway cannot hide a later invocation. The
   gateway records only the structural request and the coordinator records its
   source acknowledgement; dispatch alone is never treated as proof of visibility.
   Full and Play declare the same gateway. This follows Android's current
   [assistant-role qualification](https://developer.android.com/reference/androidx/core/role/RoleManagerCompat#ROLE_ASSISTANT)
   without adding an always-resident voice-interaction service.
2. **Turn on** runs one structural activation coordinator. It probes current
   evidence and opens only the next missing app-owned permission request or
   Android-owned settings surface. On return it re-probes and continues. It
   never shows a capability checklist, setup dashboard, or user-facing self-test.
3. The required activation path is Gemini API configuration, microphone,
   foreground notification where Android exposes that runtime permission,
   Accessibility, display-over-other-apps for the orb, and a fresh
   whole-display MediaProjection grant. The projection request is the final
   Android-owned step before service start and is requested for every session;
   its token is passed once to the foreground service and is never cached or
   reused. When the key is missing, the coordinator shows one short toast and
   opens SGT's existing Settings section, where provider credentials already
   live. It does not own a second credential form. Android grants stay in
   Android-owned surfaces. A refusal or unresolved grant stops that activation
   attempt without looping or claiming success, and another **Turn on** starts
   from fresh evidence.
   Accessibility readiness requires both Android's configured-service record and
   a live `SgtAccessibilityService` binding. The configured record alone is not
   semantic or control authority. When Android reports the service configured
   but the binding is absent, activation waits only for a bounded reconnect
   interval and re-probes both facts. A recovered binding continues without
   navigation; a freshly disabled service opens the Android-owned Accessibility
   surface; a still-configured service stops that activation attempt without
   opening a surface that offers no unresolved grant. Another **Turn on** starts
   from fresh evidence. It never starts a degraded session or claims readiness.
4. Once required evidence is sufficient, SGT starts the correctly typed
   foreground service, creates the granted projection and its virtual display,
   then starts the runtime and orb. The card becomes **Turn off** and remains the
   primary stop control; the ongoing notification also retains a Stop action.
   If projection creation fails or Android revokes the session, Phone Control
   stops instead of continuing with reduced visual authority.
5. The first orb appearance asks for a power preference in a compact, orb-owned
   prompt: standard Android, SGT Bridge, Shizuku, or root. The choice is an
   authority selection, not a suggestion: standard disables elevated providers,
   SGT Bridge selects only the first-party authenticated ADB route, Shizuku
   selects only the Shizuku shell route, and root selects only the root route.
   The chooser is a compact four-choice card without explanatory prose. Purple
   fill marks only the currently persisted authority choice. It marks SGT Bridge
   with a star as the recommended non-root route even when another choice is
   selected; recommendation alone never receives the selected fill or a distinct
   border. The star is the only recommendation styling. This is presentation,
   not an automatic selection or a weaker fallback contract. A
   paired bridge exposes only a compact secondary forget action. Forgetting
   deletes the app-owned key and pairing state, persists standard authority, and
   rebuilds the chooser from that persisted choice.
   Choosing Shizuku persists that requested authority before setup starts and
   keeps a resumable setup session pending until Shizuku is ready or the user
   selects another authority. SGT explains the next unavoidable user action,
   uses only a short localized state label (at most 32 rendered characters) in
   transient toasts, keeps a compact,
   non-obscuring status in the orb, and keeps the full persistent instruction in
   the ongoing notification. Default English, Korean, and Vietnamese resources
   own these strings and resolve from SGT's in-app language setting, including
   toasts, the orb chooser, runtime status, and notifications. Every other UI
   locale uses Android's default-resource fallback. SGT opens the official store route when
   Shizuku is absent, observes package
   installation, opens the installed manager, re-probes on package, activity,
   and Binder events, then requests SGT's Shizuku grant as soon as the Binder is
   ready. The orb selection is itself a user-originated setup goal: while the
   normal live turn boundary is idle, send that provider goal through the same
   full-catalog semantic/vision agent used for ordinary Phone Control. The model
   automates reversible navigation on the visible setup surfaces, including
   exposing the exact Android/provider input or confirmation surface. It stops
   before reading, filling, submitting, or approving an installation
   confirmation, pairing code, credential, trust decision, or any other
   system-owned checkpoint so the user can perform it. A busy live turn is never
   interrupted or raced; the setup
   goal remains bounded and pending until the turn is idle. It must not end setup
   merely because one external return has the same probe state, require the user
   to select Shizuku again between stages, repeatedly reopen the same external
   surface, or replace the ordinary tool catalog with a provider-specific click
   script. This app-originated setup turn is structurally silent: generated
   assistant audio and captions are discarded, and the whole internal turn is
   excluded from conversation memory. It also cannot replace the setup-owned orb
   caption, state, or icon with thinking, tool, or done presentation. Silent
   ownership is registered before its payload is sent, so even an immediate
   provider response cannot cross onto the conversation surface. Tool calls and
   lifecycle completion still run normally.
   Provider setup automation is single-flight. A lifecycle return or repeated
   capability callback for the same selected provider updates public guidance
   and may strengthen the pending protected handoff, but it reuses the original
   goal owner instead of queueing another model turn. A different provider
   cannot inherit that owner. Completion is matched to the original goal ID, so
   coordinator reentry cannot delay or steal its protected-checkpoint handoff.
   If the user interrupts it, normal conversation presentation is restored
   before the new user turn is admitted. Android/Play installation
   confirmation and Android-owned
   wireless-debugging pairing, trust, and confirmation remain user actions; SGT
   advances every surrounding step. If Android's screen-share protection hides
   a private notification action, secret field, or equivalent checkpoint, the
   bounded agent goal finishes at the nearest visible surface. SGT then suspends
   every model-visible pixel and semantic observation and drains queued visual
   evidence while the live socket, microphone, audio, orb, and conversation
   remain active. The selected provider declares the structural capture policy:
   SGT Bridge retains the existing MediaProjection session because its local
   pairing exchange does not require notification interaction; Shizuku releases
   MediaProjection because its protected notification reply cannot reliably be
   completed while screen sharing is active. A public-version setup
   notification keeps the instruction visible. Entering the checkpoint
   immediately replaces navigation guidance with neutral setup progress, and
   late external progress callbacks cannot publish guidance or queue another
   setup goal while the checkpoint owns the runtime. A provider adapter may relay an
   ephemeral one-time value only inside this sealed checkpoint after the user's
   explicit provider selection. The value never enters model context, captions,
   logs, screenshots, traces, storage, or generic tool results. Structural
   ambiguity or relay failure becomes an honest typed user step. A retained-
   projection checkpoint resumes model-visible evidence on the already attached
   virtual display, then performs the same fresh provider probe; it never asks
   for redundant screen-share consent. A released-projection checkpoint replaces
   the pre-checkpoint setup guidance with the immediate fresh-screen-share step,
   brings the existing transparent coordinator above any coordinator-owned
   external surface, and asks for a fresh MediaProjection grant. If that
   coordinator task no longer exists, Android creates it. Merely
   dispatching the activity intent is not proof that the prompt opened. SGT uses
   an explicit immutable internal PendingIntent with the applicable Android
   background-launch opt-in; the exact coordinator launch token must be
   acknowledged before the projection launcher dispatches the system prompt.
   Once that reentry is pending, any result from the retired external setup
   surface is discarded and cannot finish the coordinator or clear setup.
   Launcher dispatch is not reported as prompt visibility; the Android activity
   result and a fresh projection attachment are the completion receipts. The
   ongoing notification is the fallback affordance. A process
   restart discards the transient checkpoint
   and runs normal activation while retaining only the user's authority choice.
   Provider setup cannot continue before the retained projection is safely
   unsealed or a fresh grant attaches to the same runtime; no command, frame,
   secret, or consent token is replayed. A released-projection relay uses a short
   localized toast before Android asks for the fresh whole-screen grant; it never
   asks the live model to narrate credentials or setup instructions. After a
   completed relay restores visual evidence, SGT makes one fresh
   probe of the selected provider. A provider that is now ready clears pending
   setup without reopening it, immediately shows one localized ready caption and
   short toast, then returns to the ordinary idle/listening cycle without an
   extra model turn. A remembered authenticated provider may initially report an
   in-flight cold reconnect. Tools that require that selected provider await the
   single bounded reconnect and use its terminal receipt; they never fail from
   the stale pre-reconnect snapshot while the same process becomes ready in the
   background. Readiness diagnostics distinguish endpoint discovery, transport,
   and rejected authorization without recording an endpoint, key, or pairing
   code. A provider that is not ready resumes
   the selected setup from fresh evidence. A relay that still needs a user step
   or fails keeps the provider
   selected and its guidance visible, but must not automatically republish the
   identical setup goal. It retries only after an explicit user action or fresh
   capability evidence.
   The first-party bridge also treats an unchanged Android Settings return as
   no progress: it probes once, remains selected and pending, and never reopens
   the same surface until fresh provider evidence or an explicit retry. Model
   completion alone cannot enter its protected checkpoint. A local structural
   probe must first prove the current pairing surface is present without
   exposing its one-time value. If a silent navigation generation ends on an
   intermediate surface, SGT keeps the original setup deadline, takes fresh
   structural evidence, and submits a bounded continuation generation. It
   never converts an unverified model completion into setup success. Exhausted
   continuation or deadline budgets retire the hidden owner, clear stale orb
   guidance, and leave the selected provider honestly pending for an explicit
   retry. Missing Accessibility, Settings ownership,
   pairing structure, code availability, pairing endpoint, transport, and
   authority are distinct typed stages. Pairing plus the initial connection use
   one monotonic end-to-end deadline rather than restarting a full timeout at
   each stage. Debug and release packages follow this same state machine while
   retaining their platform-required per-UID keys, grants, and pairing state.
   Protected setup navigation is checkpoint-driven rather than turn-end-driven.
   Each provider supplies a stable semantic setup contract; the local structural
   checkpoint monitor is armed while its silent navigation goal is active. When
   the exact protected surface appears, SGT immediately seals model-visible
   pixels, blocks new model tools, lets the one owned action settle, retires the
   hidden generation, and only then starts the local secret-handling adapter.
   Ordinary `done` semantics remain unchanged. Android Settings entry uses only
   documented public actions; private components, localized-label routes, and
   OEM-specific navigation are forbidden.
   An accepted authority-setup session emits one short localized app-owned voice
   announcement before automation proceeds. While that structural session is
   active, microphone capture may remain allocated for runtime continuity, but
   its samples and level activity are discarded locally and never reach the live
   model. Success emits one localized completion announcement, retires the
   internal setup generation, clears setup-owned captions and playback, and
   opens a fresh non-resumed live protocol session. Microphone input becomes
   admissible only after that fresh session is ready and the local completion
   announcement has ended, so setup speech, ambient user speech, tool context,
   and model output cannot leak into the first normal user turn. Cancellation
   or bounded setup exhaustion performs the same clean
   session boundary without a success announcement.
   Tapping the orb reopens the preference prompt, so the user can explicitly
   cancel the pending route by choosing another authority. If that choice occurs
   while a protected checkpoint is active, SGT cancels the old local adapter and
   keeps the runtime sealed until capture is reconciled. It immediately unseals
   a retained projection, or requests a fresh MediaProjection grant only if the
   old provider released it, then starts the newly selected authority setup.
   The notification cancel action selects standard authority and follows that
   same policy-aware route, so it cannot strand the live runtime behind the
   visual gate. An abandoned first-party pairing call remains singly owned until
   its bounded terminal return, then forgets its client key before another
   pairing can begin. It never queues a provider automation goal while model
   tools are blocked.
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
System-assistant requests additionally record the structural route, gateway task
identity, and whether coordinator dispatch was requested. A subsequent
coordinator open or re-entry carries a bounded source acknowledgement, allowing
request-versus-arrival diagnosis without persisting assist context.

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
- Transport resumption may preserve authenticated conversation context and
  queued control receipts, but it does not own a disconnected output
  generation forever. When no accepted tool effect is still settling, a
  transport interruption retires the local generation before reconnect. Late
  resumed output cannot reopen it, and the ready connection returns the orb to
  listening instead of leaving an orphaned working state.
- A synchronous function call is answered before any tool-owned screen evidence
  or ambient screen frame is sent. Ambient video remains paused while that call
  is unanswered; microphone audio remains live so barge-in still works. Each
  outbound flush sends its bounded microphone burst before queued control and
  visual payloads. This priority changes latency only: the tool response still
  precedes its tool-owned screen evidence, and exact tool evidence still
  precedes ambient video. Transport diagnostics keep only a bounded structural
  tail (payload kind, byte count, protocol phase, and pending-work count); they
  never retain payload content.
- `done` is terminal. Accept it once, release its current-generation audio once,
  retire later tools/output, run silent local cleanup, and return idle.
- If a generation ends without `done`, release its one response and return idle.
  Never synthesize a continuation, verifier turn, or completion quorum.
- Barge-in monotonically cancels only the owned turn/generation/jobs. Late
  callbacks from retired generations cannot act, speak, reopen the turn, or
  resurrect a reconnect.
- Interrupted mutations with no proven no-effect receipt block later mutation
  and completion until a fresh observation reconciles state.
- Two equivalent structured failures for the same normalized tool request and
  current surface retire that retry path. A third identical call returns
  `repeated_failure` with proven no effect instead of dispatching. The fingerprint
  uses tool identity, canonical structured arguments, typed failure code, and
  observation identity only; it never matches user phrases or app content.
  Verified effects and genuinely fresh evidence clear the relevant history.
- A mutation-requested screen refresh clears that reconciliation gate only after
  the fresh frame is successfully transmitted. It may release a generation whose
  completion was already deferred, but it never completes an active generation
  early and an unsent capture proves nothing.
- Automatic reconciliation is internal turn state, not a user-facing failure.
  While the fresh frame is pending, preserve the current working/finalizing orb
  state and caption. Only a bounded capture failure may publish a degraded
  screen-capture status; a normal successful action must never flash an error.
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
The host supplies a bounded device execution deadline that is shorter than its
own receipt deadline. The receiver clamps that value to the shared harness
bounds, then retains admission through timeout cancellation and terminal
settlement. A host timeout therefore cannot silently become an unrelated fixed
device timeout or release a still-running production operation.

### Speech and captions

- Caption and audio belong to the same generation.
- Microphone capture remains open while assistant audio plays and while tools
  run. Local RMS is presentation/activity evidence only; it never phrase-gates
  input and never retires a playing generation. Gemini Live's typed
  `interrupted` event is the sole speech barge-in receipt that retires that
  generation, stops its playback, and cancels only its owned work.
- Use the Windows PCM16 voice floor of 120 RMS for local activity. On each new
  locally voiced burst while assistant playback is quiet, request one fresh
  screen frame immediately so pixels lead the utterance. Keep the 500 ms
  structural hangover independent of Gemini Live's provider-owned automatic
  activity detection. Structural burst ownership is one-to-one: if a new
  above-threshold burst begins after the hangover before the prior burst's
  below-threshold sample was observed, close the prior epoch before opening the
  next one.
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
- Orb audio receives the same signal as Windows: normalized PCM16 RMS is treated
  as voiced at `120 / 32768`, multiplied by `32768 / 4000`, clamped to `[0, 1]`,
  and otherwise sent as zero. This is a visual mapping only and cannot affect
  upload, interruption, tool dispatch, or semantic state.
- Android renders the caption in that same canonical HTML, including Google Sans
  Flex, transparent background, white text/shadows, placement, and incremental
  word motion. It must not maintain a native substitute caption or restart
  unchanged words as streaming text grows. The renderer becomes visible only
  after that face is ready or a bounded readiness diagnostic expires. Android
  serves its existing shared WebView product-font asset from the app-owned asset
  origin; Phone Control must not carry a duplicate font payload or remain hidden
  because font-readiness reporting failed.
- The canonical visual renderer is a full-display, non-touchable trusted
  accessibility overlay; a separate orb-sized touch shim owns drag/tap input.
  This preserves exact pixels while underlying apps remain interactive. During
  an accessibility-service reconnect, an application-overlay fallback may render
  only at the platform-reported non-obscuring alpha until the trusted host returns.
  Both hosts resolve the default physical display explicitly; the MediaProjection
  virtual display is a capture sink and never a visual-overlay destination.
- Android follows the shared
  `parity-fixtures/android-webview-overlays/rendering-contract.json`: the overlay
  window owns hardware acceleration, latest visual/drag work is conflated to one
  display-frame dispatch, dragging never relays out the unchanged full-display
  renderer window, and normalized position is persisted only at gesture end.
- The renderer's canonical `orbRegion` message is the only capture-exclusion
  geometry. It covers the liquid body, full glow, and current caption. Android
  scales that region from the renderer viewport into display coordinates and
  keeps it separate from the orb-sized touch shim. Caption growth can enlarge
  capture exclusion but can never enlarge the touch-consuming surface.
- Crossing the touch shim's drag threshold opens the same shared, single-target
  bottom dismiss bubble used by Android's other floating overlays. Current raw
  pointer coordinates drive its proximity feedback. The target uses the orb
  renderer's current window owner and type and is attached above that renderer;
  an attachment failure is never represented as a visible target. Releasing
  inside its commit threshold runs the canonical orb exit plus the shared
  swallow animation, stops the Phone Control foreground service, and returns the Apps card to **Turn on**.
  Releasing elsewhere hides the target and persists the clamped orb position;
  cancellation hides the target without stopping the session. The dismiss target
  is local overlay chrome, not a model tool or phrase-gated action.
- A capture or tool action must never blink the orb. Window-scoped capture does
  not mutate it. An action aimed through its footprint moves it clear instead of
  fading it. A receipt that both reports uncertain effect and carries a fresh
  reconciled observation is reduced atomically and never exposes an intermediate
  warning state.
- The orb caption is conversation presentation, not a diagnostic console.
  Provider, transport, capture, contract, retry, and reconciliation error text
  stays in typed receipts, the journal, Logcat, and the notification. A degraded
  runtime preserves the current conversational orb state instead of flashing an
  error glyph; its caption is empty unless there is one explicit user-owned
  Android step with localized guidance. Publishing the same visual state twice
  is a no-op.
- While the current generation is in `responding`, classify at most the latest
  600 caption characters about once per second through the same Taalas client
  used by Windows and map its exact label to the canonical `sentiment_*` glyph.
  This is conflated background presentation work: it never delays speech, tool
  dispatch, or turn completion, and any network or malformed response keeps the
  current icon silently.

### Foreground execution

- A listening/working Phone Control session runs as the correctly typed visible
  foreground service with an ongoing notification and the user-visible
  orb/overlay in both distributions.
- Start the session from a user-visible action. Do not depend on hidden
  background activity launches or an immortal background daemon.
- The foreground service includes the media-projection type before it consumes
  the one-shot grant. Runtime and orb startup require a successfully created
  projection virtual display. Projection callback stop, lock-screen revocation,
  or replacement by another projection terminates Phone Control; it never
  remains listening with a dead capture grant. Microphone and playback service
  types follow the platform contract already declared by the mobile app.
  Process death retires owned jobs and requires a fresh consent session; it
  never replays a command or reuses projection consent.
- Projection callbacks, reader/display retirement, resize, and frame ownership
  are serialized on the projection handler. Stop first closes admission and
  settles any pending capture, then retires platform resources after any
  in-flight callback. A callback never reads an `Image` after close, cleanup
  exceptions never escape the callback thread, and display metadata comes from
  the structural default-display service rather than an Activity-only context.
- Android may redact private notification content while whole-display capture is
  active. Provider setup may therefore declare a protected user checkpoint.
  The full-catalog agent first completes its bounded reversible navigation goal.
  Only after that goal, its tool receipts, reconciliation, and queued speech
  settle at either quiescent turn phase (`idle` or `listening`) does the service
  atomically block model-visible semantics and pixels and drain pending visual
  payloads. The provider adapter declares whether its platform interaction can
  retain the attached projection or must release it. The runtime, socket,
  microphone, audio, orb, tool state, and conversation remain alive. The setup
  coordinator keeps durable public guidance, observes capability state, resumes
  the retained projection directly, or requests a fresh MediaProjection grant
  only after a released-projection step. A provider adapter may perform
  a bounded local relay of an ephemeral one-time value only while the visual gate
  is sealed. It receives no model plan or arbitrary target text, uses structural
  provider identity, exposes no secret-bearing result, and clears transient
  material immediately. Its ongoing notification has an explicit cancel action
  which cancels the adapter, selects standard authority, and either unseals the
  retained projection or requests fresh projection consent, so a suspended setup
  can never trap the selected authority.
  This is one provider-neutral lifecycle; provider glue cannot weaken it with
  localized UI text, coordinates, screenshots, or model-visible secret handling.
- A retained projection reopens visual evidence only after the local protected
  adapter has terminated and checkpoint ownership is retired. A fresh projection
  grant attaches to the existing live runtime and reopens visual evidence only
  after a virtual display is ready. Denial keeps the runtime in its explicit
  capture-suspended state with public resume/cancel affordances.
  A capture-resume reentry retires any prior coordinator-owned external surface
  above the transparent coordinator before launching the system capture prompt;
  stale external activity results cannot finish or redirect the new step.
  Pre-checkpoint guidance is session-owned and is replaced as soon as the
  checkpoint outcome makes fresh projection the next structural step. It is
  cleared after the post-attach provider probe reports ready.
  Unexpected projection callback stop, lock-screen revocation, replacement by
  another projection, process death, or a capture loss outside that planned
  checkpoint still terminates Phone Control and retires owned work.
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
| `observe`, `act`, `do_steps`, `click_at`, `look`, `click_target`, marks, zoom/view tools | Accessibility windows/nodes first, then explicit current-frame Gemini grounding. A requested vision tool remains valid on a structured surface |
| `type_text`, keyboard, scroll, drag, pointer tools | Node actions and Accessibility input connection first; gesture dispatch or a proven elevated input backend as fallback |
| `open_url`, `launch_app`, window/app focus/list tools | Intents, package/task/display state, Accessibility global actions, and elevated system APIs when required |
| `system_query`, files, clipboard, and `run_command` | App APIs and persisted SAF grants first; selected SGT Bridge, Shizuku, or root authority for operations the app UID lacks. Shell commands do not require an Accessibility surface lease |
| artifacts, memory, paste/extract/save tools | The shared artifact/memory schema with Android content URIs and persisted grants |
| browser read/navigation/tabs/eval/network/console/upload tools | Direct integration when available; credentialed browser CDP for full page authority; Custom Tabs for shared browser sessions; Accessibility for browser chrome/fallback; owned WebView only for SGT-owned or deliberately isolated content |
| research and web search | Shared research/source/evidence contract; research-owned tabs are turn-scoped and silently cleaned |
| app integrations and MCP | Shared declaration, schema, lifecycle, and typed failure contracts; platform transport is thin glue |
| `done` | The canonical terminal turn signal |

Structured tool arguments are protocol identities, not descriptive hints.
Provider planning resolves a tool's capability from its validated structured
arguments before dispatch. In particular, `act(fill)` plans and validates
`ui.text_edit`, while pointer verbs plan `ui.pointer_action`; a static tool-name
classification cannot reject a correct handler receipt.
`type_text` and `key_combination` require the complete exact current target
returned by `list_windows`; an app label or window title is not a substitute.
If no current target is available, call `list_windows` before dispatch.
Numbered grid cells belong only to the latest visual frame. Android `observe`
refreshes semantic `@id` identities but does not itself renew that visual grid,
so focused-surface scrolling should omit `cell` unless the current numbered
frame supplies one. A stale optional cell must fail closed rather than silently
changing the requested coordinates.

`system_query` exposes exactly the canonical Windows pairs:
`capabilities.list`, `audio.active_sessions`, `clipboard.text`,
`process.list_basic`, `storage.volumes`, and `window.list`. Android rejects any
other pair at the tool-contract boundary before selecting a provider. Providers
must not add aliases that make the same stable catalog mean different things on
each platform.

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
`@id` actions. Window topology and a mutation accepted through Phone Control
invalidate the generation immediately. Ambient Accessibility click, scroll,
text, selection, focus, and generic content notifications can be emitted by
animation, lazy layout, another input source, or the controller's own overlay;
they advance a separate visual revision instead of retiring every semantic
lease. Every semantic mutation still resolves the live node path and exact
fingerprint immediately before dispatch. Coordinate actions require the visual
revision captured with their grid or visual-grounding verification, so content
churn cannot turn an old image into a click. For actions that cross a remote
grounding request, the provider takes one final fast screenshot and renews that
revision only when the hard surface lease is unchanged and every selected
target's local pixels still match its bound signature. Unrelated ambient changes
outside those target regions therefore do not discard a valid target; a changed
target, hard generation, surface identity, or revision during final dispatch
still fails closed. Streaming may return the bitmap captured at one instant
while content continues changing. A topology event or an explicit controller
mutation during image capture returns `stale_frame`. Explicit visual
tools use only their bounded internal retry; ambient streaming immediately uses
the lease-free projection fallback described below.
Ambient capture never changes controller overlay alpha, interactivity, window
membership, position, or layout. A whole-display provider instead masks the
bounded controller-owned region in the captured bitmap after capture. A
window-scoped provider excludes controller windows by selecting the external
window itself. Therefore continuous visual evidence cannot visibly blink the
orb, steal touch input, create topology events, or invalidate its own frame and
action leases. A short-lived overlay relocation is permitted only at the final
dispatch edge when the requested pointer path actually intersects the
controller-owned interaction region; it is never a periodic capture strategy.
Accessibility gestures and every elevated pointer backend use that same final
dispatch wrapper. Controller-overlay window events are recognized by durable
controller window identity even when Android omits the event package, and never
retire leases for the external surface.

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

Android visual observation requires two complementary providers on the default
display. MediaProjection owns the live whole-display session and supplies
whole-display pixels or a typed fallback when an Accessibility screenshot route
cannot capture the requested pixels. Accessibility remains the preferred
active-window route because its exact window identity excludes controller
overlays without mutating visible UI and binds pixels to the semantic surface
lease. On API 34+, active-surface frames use the exact Accessibility window id
with `takeScreenshotOfWindow`. The API 30-33 compatibility route uses a bounded
display capture followed by an in-memory controller-region mask because Android
exposes no older window-scoped screenshot API. Normal
frames carry the same numbered 6x5 grid geometry used by `click_at`, `drag`, and
`zoom`. Every grid and crop is bound to the observation generation, display,
window, package/surface, rotation, density, capture timestamp, absolute screen
crop, visual-content revision, and exact capture provider. `zoom` accepts only a cell from the current frame and magnifies that
cell with one-quarter-cell context; a changed generation returns `stale_frame`
with proven no effect. `reset_view` captures the fresh active application
surface. `see_whole_screen` captures the complete default display and reports
that display scope rather than implying unavailable multi-display pixels.
MediaProjection row decoding copies exactly the visible pixel width of each row.
It accepts row padding without requiring padding bytes after the final visible
row, validates all visible bytes before allocation, and reports only typed
geometry and exception-class diagnostics on failure.

A transient Accessibility disconnect, unavailable surface, unstable semantic
tree, or frame-generation race does not discard an attached MediaProjection
session. Ambient streaming makes one semantic capture attempt. On API 34+ it
then tries one lease-free screenshot of the current external window, so semantic
tree churn cannot expose controller pixels or interrupt live vision. Only when
that window-scoped route is structurally unavailable may it use the
whole-display projection frame with the complete canonical `orbRegion` removed.
It does not retry Accessibility traversal inside the ambient frame interval.
This keeps the live model visually current without periodic retry churn and
reports Accessibility as the unavailable action authority. When Accessibility
stabilizes, the next successful semantic capture restores leased active-window
frames and numbered-grid authority. Lease-free and projection-only pixels must
never be presented as proof that an Accessibility mutation can be dispatched.

An API 34+ window id may expire after observation but before the platform
accepts its screenshot request. That typed invalid-window result is a capture
race, not a broken screen provider: the same capture operation retries once
through the display-scoped Accessibility route, preserving the requested
absolute crop and masking only the controller overlay in the captured bitmap.
Explicit visual tools retain a bounded two-attempt stale-frame grace period.
Ambient streaming uses projection-only continuity instead of repeating semantic
capture. A third consecutive remaining stream failure becomes visible
degradation, while one successful transmitted frame clears the failure state.
Non-retryable failures remain visible immediately.
This recovery depends only on typed platform outcomes and never on the current
app, surface text, coordinates, or user language.

`look` does not run a second language agent or synthesize a reading in code. It
places one clean, ungridded capture of the current view into the same live
model's input before the tool receipt and reports exact frame metadata; the live
model owns the requested visual meaning. Secure capture, display mismatch,
provider loss, and screenshot rate limits remain typed failures. `point_at`
stays unavailable because Android has no universal persistent touch pointer or
hover state. `drag_target` selects both described endpoints from one immutable
clean frame in one grounding request. It verifies a fresh crosshair crop for
each endpoint and performs one Accessibility swipe only after final
exact-surface and target-local pixel revalidation.

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

An elevated provider is effect authority, not a perception system. For pointer
and key actions, SGT Bridge, Shizuku, or root may run only after the narrower
Accessibility or DOM effect path proves no effect and only against the exact
current observation or visual lease. The explicit model-chosen vision tool still
grounds coordinates. A ready ADB bridge never authorizes blind
coordinates, stale geometry, or a silent change of requested tool.

### Baseline Accessibility backend

The existing service already retrieves interactive windows and can capture the
display. Phone Control additionally requires:

- general immutable window/node snapshots rather than text-selection-only
  traversal; API 30+ enumerates all displays, API 29 declares its default-display
  limit and must never call the API-30-only accessibility-window display-ID
  accessor, and a window snapshot survives absent/inaccessible root content
  with `content_accessible=false`;
- session-scoped window/content/scroll event subscriptions sufficient to
  invalidate snapshots, narrowed again while idle; events are hints, never a
  substitute for a fresh observation;
- `android:canPerformGestures="true"` before using `dispatchGesture()`;
- global actions such as Back, Home, Recents, and notifications where supported;
- node click, focus, set-text, selection, scroll, expand/collapse, and other
  advertised `AccessibilityAction`s;
- actionable containers sometimes expose their human-readable semantics only
  on non-actionable descendants. For a small, completely traversed subtree,
  an otherwise unlabeled action owner inherits bounded, deduplicated,
  model-visible labels from safe non-actionable descendants while retaining
  the ancestor's real lease and bounds. This is structural and
  language-neutral: it never matches a phrase, resource ID, device, OEM, or
  screen. Editable or protected descendants, incomplete/deep subtrees, and
  nested action owners are not inherited. Secret text therefore cannot be
  promoted into an ancestor, and dispatch still revalidates the exact action
  owner rather than climbing from an arbitrary label;
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

### Visual grounding

- Semantic Accessibility/DOM state remains first choice.
- A screenshot is bound to its exact display/window, crop, rotation, density,
  insets, snapshot generation, and capture timestamp.
- Gemini grounding receives one clean current-view image. It may locate one
  named point, enumerate up to 30 relevant actionable marks, or locate both drag
  endpoints in one request. The strict model-neutral JSON point collections and
  0-1000 `x`/`y` coordinate grid are owned by
  `parity-fixtures/phone-control/model-chain.json`; malformed, duplicate,
  unknown-ID, or out-of-range output advances the grounding chain and otherwise
  fails closed. The caller always supplies the matching bounded schema; each
  endpoint profile selects strict-schema, JSON-object, or prompt-only transport.
  One outer JSON Markdown fence may be removed before the same strict local
  validation, while surrounding prose remains invalid.
- Visual surface selection must not require an Accessibility node root. A
  rootless active application/system window may recover its package authority
  only from an exact-window Accessibility event recorded in the current
  observation generation. It must never infer authority from titles, visible
  text, coordinates, model output, shell dumps, or device-specific window
  formats. Without that structural attribution, visual reading may continue but
  mutation returns `surface_authority_unknown`.
- `map_targets` converts the model's points into numbered current-frame anchors.
  Every anchor binds the exact frame identity, surface lease, point, semantic
  label, and target-local visual signature. `click_mark` never asks the model to
  reinterpret an old frame: it requires the same hard surface plus a fresh
  matching local signature immediately before input.
- `click_target` keeps canonical single-tool behavior. One model request locates
  the named point on a clean frame; a fresh crosshair crop must confirm that its
  center is inside the requested target with at least 70% confidence; final
  surface and target-local pixel revalidation must then pass before dispatch.
  Verification accepts only the schema-valid JSON object owned by the shared fixture;
  prose, missing/extra fields, or out-of-range confidence advances the chain,
  while a well-formed rejection is terminal and no later model may overrule it.
  Language meaning never moves into Kotlin or Rust and no second agent tool call
  is required.
- `drag_target` locates `from` and `to` in one call against the same clean frame,
  rejects missing, duplicate, or zero-distance endpoints, verifies each using a
  fresh crosshair crop, and dispatches one leased swipe only after both local
  signatures and the exact surface still match.
- Visual grounding requires no local ML asset, automatic blind-frame inference,
  model download, or Play/Full delivery difference. ORT remains only where
  another subsystem independently needs it.

## Optional Authority Providers

### Ordinary Android and special-access APIs

Use normal APIs before elevated bridges: intents, ContentResolver, MediaStore,
persisted Storage Access Framework grants, MediaSession, Notification listener,
runtime permissions, overlay access, and other user-granted special access.
Persisted URI grants are first-class resources, not filesystem-path guesses.

Canonical `list_files` standard-folder identifiers are resolved structurally
before provider selection. Android maps `home`, `documents`, `downloads`,
`music`, `pictures`, and `videos` to the current primary shared-storage
directories and treats an unavailable platform directory as a typed result;
unknown relative paths never become working-directory guesses. SGT tries the
ordinary app/SAF route first, then the exact selected elevated provider in the
shared `file_resource_access` order. The elevated implementation uses bounded
literal arguments and a fixed read-only listing program, reports truncation
instead of returning a partial listing as complete, and never exposes arbitrary
shell interpretation through `list_files`.

`launch_app` resolves its input by structure, not by language or filename
guessing. Exact installed package/label inputs use the launcher API. Before
launcher dispatch, an exact active-and-focused non-controller application
surface for the resolved package is preserved and returns a proven-no-effect
receipt. This prevents a redundant launch from restoring unrelated history in
an already-foreground app; missing or ambiguous foreground evidence continues
through the normal launcher path and is never reported as preserved. Absolute
paths, standard-folder paths, `content:` URIs, and `file:` URIs use Android
resource viewing with a MIME type, temporary read grant where applicable, and
the platform chooser or owning handler. When `args` is present, it may identify
one of those resources to open with the exact app in `name`; arbitrary Android
command-line text returns a typed structural boundary instead of guessed intent
extras. The ordinary app/SAF route is tried
first; if it cannot represent an otherwise valid shared resource, the exact
selected provider in the `app_and_task_control` route may dispatch a bounded
literal `am start`. APK viewing reaches the Package Installer checkpoint and
never silently installs. Unsupported URI schemes, absent resources, missing
handlers, ambiguous app labels, and provider failures remain distinct typed
results. `open_url` stays restricted to absolute `http:` and `https:` URLs, so
neither tool silently reroutes or retries the other.

Filesystem mutations sharing one canonical path are serialized within SGT.
`save_artifact` with `overwrite:false` uses an atomic create-only operation, and
an exact text replacement revalidates its expected hash immediately beside the
atomic replacement. All replacement groups are planned against one immutable
baseline and overlapping ranges are rejected. Ordinary CSV/TSV edits preserve
the parsed record shape and formula cells before staging. The same bounded,
unambiguous repairs as Windows remove only redundant trailing empty fields,
serialize a split changed trailing value, and restore exact opaque formula
bytes; ambiguous structure still fails closed. A proposal that intentionally
drifts shape or formulas must use `edit_text_file_structure`. Its first call is
a no-effect preflight that returns a token bound to the exact format, baseline,
and proposed bytes. That token identifies the proposal but never authorizes it.

Every dedicated file write—ordinary edit, structural edit, or artifact save—
also passes an independent target-scope checkpoint. Its bounded proposal
contains the tool, canonical target identity, target existence state, and the
current user request, but never replacement or artifact content. At least two
distinct text-model verdicts must approve and any negative verdict vetoes the
write. This authorizes only the exact target scope, not the mutation content.
Starting a new independent user turn retires the completed prior turn's scope.
The independent verdict candidates come from the effective shared Text-to-Text
chain, including signed Live-feed interleaving and the normal provider/key,
cooldown, and token-budget gates. They must not fall back to an authored-only
snapshot that silently omits currently eligible models.

An authorized write receives an immutable target lease containing the canonical
path, whether it existed, and its existing hash when applicable. The commit edge
revalidates that lease. A target authorized while absent cannot overwrite a file
that appeared later, including with `overwrite:true`; canonical-target drift or
content drift is reported with proven no effect.

An identical structural retry can cross its separate private commit edge only
after another text-model quorum compares the exact structural proposal with the
same bounded current user request: at least two distinct positive verdicts and
no negative verdict are required. Language meaning remains in the models;
Kotlin uses no phrase or locale gate. Immediately before atomic replacement
Android revalidates the target lease, expected hash, proposal, token, and
structural audit. A concurrent creator or modifier is reported without silently
overwriting its bytes. The shared machine contract is
`parity-fixtures/phone-control/file-mutation-contract.json`.

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
- Selecting Shizuku immediately makes it the requested elevated provider and
  starts an event-driven, resumable setup session bounded by structural probe
  state. SGT gives short localized feedback before leaving its surface, keeps
  compact guidance on the orb and full durable guidance in the ongoing
  notification, opens only the official manager/store/download route for the observed state,
  observes installation while Phone Control remains alive, re-probes on package
  changes, external return, and Binder events, and automatically requests SGT's
  Shizuku permission once the Binder is ready. An unchanged return leaves the
  selected route pending without reopening or pretending setup ended. Provider
  code never branches on localized Shizuku labels or contains a model-facing
  private-manager click script; the normal full-catalog model may navigate the
  currently visible manager and Android surfaces. It may expose the pairing
  surface but never receives, narrates, fills, submits, or approves a pairing
  code, Android/Play confirmation, credential, or trust decision. When the
  pairing surface is ready, the general protected-checkpoint lifecycle seals
  model-visible pixels and semantics and releases capture. A Shizuku adapter may
  then relay the one-time pairing value locally between structurally identified
  Android/Shizuku surfaces. It must refuse multiple candidates, unexpected
  packages/windows, stale nodes, non-ephemeral inputs, or any need for model
  interpretation. Android/Play install, wireless-debugging enablement, trust, and
  confirmation remain user steps. After the adapter or user completes the step,
  SGT probes Binder authority and requests a fresh capture grant without ending
  the live conversation. It never claims the provider is ready before the grant
  probe succeeds.
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

### First-party SGT privileged bridge

The first-party local ADB bridge removes the external Shizuku-app dependency
when the device supports wireless debugging. It is an additional provider, not
a special agent mode. The `sgt_adb` orb choice selects only provider
`sgt_adb_bridge`; it is present in both Play and Full builds. Ship it only as
successive real-device-tested capability layers: discovery and pairing,
authenticated reconnect, scoped command service, then higher-level routes such
as Chrome sockets. A layer stays typed `needs_user_step` or `degraded` until its
real-device proof passes. Its contract is:

- bind only locally; never expose an unauthenticated LAN command server;
- keep ADB keys in Android Keystore and provide explicit revoke/forget controls;
- show Android-owned enablement, pairing, trust, and MediaProjection UI; a sealed
  local adapter may relay a one-time pairing value under the same protected
  checkpoint contract, but the model and diagnostics never receive it. This
  adapter retains the current MediaProjection session while model-visible
  evidence is sealed, then resumes that same session without another consent
  prompt;
- once the authenticated pairing exchange succeeds, persist that structural
  fact before connection-service discovery. A delayed connect remains a bounded
  reconnect state and never asks for the one-time code again or claims another
  user step is required;
- require a structurally verified current pairing surface before sealing visual
  evidence for the local relay. An unchanged Settings return remains pending
  without reopening, and each failure stage stays distinct in diagnostics;
- bound pairing discovery, pairing exchange, connection discovery, and the
  initial connection by one monotonic end-to-end deadline;
- bind pairing and connection discovery to the same persisted Android ADB mDNS
  identity family. Accept exact names and Android's documented pairing/connect
  backend-suffix variants; never accept an unrelated `adb-` family merely
  because its address belongs to this device;
- authenticate the app/bridge endpoint, scope each job, and return effect receipts;
- discover only Android's local mDNS pairing/connect services and reject remote
  addresses that do not belong to a current local interface;
- survive process interruption without replaying commands;
- start the private bridge process with only its service-owned runtime. The full
  application container belongs exclusively to the primary application process;
  dedicated service and worker processes must not initialize UI, TTS, creation,
  WebView, or unrelated background work. Binding timeouts report a genuine
  bridge failure and never mask unrelated application startup;
- expose a provider-neutral probe, start, cancel, and revoke API so Shizuku,
  first-party ADB, root, and future backends share one router;
- keep command transport typed and bounded; never expose a generic unauthenticated
  shell listener or infer authorization from command text;
- terminate each command on its exact per-operation status-marker line rather
  than waiting for transport EOF. The complete line terminator is part of the
  terminal signal, so a split numeric status cannot be accepted early;
- accept shell authority only from a successful `process_exited` receipt that
  proves process start, exit code zero, no timeout, no cancellation, shell
  provenance, and exact UID 2000 output. A marker observed in a timed-out or
  cancelled receipt never authenticates the provider;
- require Android 17 `ACCESS_LOCAL_NETWORK` when applicable.

Android 17 plus adb 37 can automatically reconnect a paired device to a trusted
workstation network. That does **not** prove that an on-device bridge or Shizuku
service auto-starts after reboot. Reboot recovery stays a typed capability state
until the exact device/provider re-establishes authority. See
[ADB Wi-Fi 2.0](https://developer.android.com/studio/run/device)
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
     semantic fallback, then current-frame vision grounding;
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
  duplex stream. The implemented routes are the first-party SGT Bridge and a
  Shizuku user service; both terminate an ephemeral authenticated loopback
  lease into Chrome's abstract DevTools socket. Ordinary root command authority
  is not evidence of a safe bidirectional stream and remains typed unsupported
  for CDP until such a transport is implemented and proved. USB/wireless-
  debugging trust remains an Android-owned user step. Never expose a remote-
  debugging endpoint on the LAN, reuse another app's pairing material, or treat
  a reachable socket as ownership of every tab.
- Opening a Custom Tab and discovering that same surface as a CDP target are two
  separately evidenced transitions. Before launch, record the exact opaque page
  target IDs and URLs. After the accepted launch, bind only one page target whose
  identity or URL changed from that baseline and whose normalized URL exactly
  matches the requested URL. Zero or multiple candidates never become a guessed
  binding. Target discoverability must be probed across supported
  browser/version combinations. If the exact target is absent or ambiguous,
  keep the authenticated Custom Tab and route only semantics that Accessibility
  can honestly preserve; CDP-only tools return `capability_unavailable`.
- SGT-owned WebViews use a direct, authenticated JS/native bridge and stable web
  surface identity. Third-party WebViews are CDP-visible only when their owning
  app enables WebView debugging. Otherwise use Accessibility and vision and
  report CDP-only tools as unavailable.
- Browser targets include browser package/profile scope, credential-context kind
  without secret material, tab/target ID, document ID, loader/navigation
  generation, frame/surface, and observation generation. A Custom Tab launch is
  not proof of a CDP target, and a visible URL/title match alone is not ownership.
- Research-owned and disposable tabs follow the Windows turn-lifetime contract.
  Android bounds turn-owned CDP targets, closes them at turn retirement, verifies
  their opaque IDs are absent, and retains unresolved ownership for later cleanup
  instead of reporting an unverified close as complete.
- `research_web` is a read-only public-network adapter and does not require an
  elevated authority or a credentialed browser. It uses isolated requests,
  blocks private/link-local destinations and credentials in URLs, preserves the
  shared source-policy/evidence bounds, and never claims browser-session state.
  Credentialed or interactive page tools remain on the browser ladder above.
- SGT Bridge and Shizuku CDP transports use the same bounded tunnel contract.
  Each lease owns an ephemeral loopback listener; every connection presents a
  random per-process bearer secret delivered only through the selected
  non-exported authority channel, and the tunnel strips it before proxying one
  stream to `localabstract:chrome_devtools_remote`. Neither the secret nor the
  lease is logged or persisted. Forgetting SGT Bridge, Shizuku service death,
  provider replacement, or service teardown closes that provider's tunnels and
  target sessions. A reachable loopback port without the secret proves no
  authority.
- Chrome target enumeration establishes opaque target identity; it does not
  silently select by title or URL. Opening a target binds the returned exact
  target ID. Switching an existing target requires the numeric handle from the
  current authenticated `browser_tabs` observation, followed by a fresh target
  probe. Navigation retires the prior document generation.
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
  surface, a live modal window above an application during an active opaque
  platform user-step session, or the active resolved handler package owned by
  that exact session may assign that state. A full-screen handler stays
  `os_owned_user_step` only for that token's lifetime; the same application is
  routine outside the session. Coordinator-owned Settings navigation may
  scroll, open the exact app row, and return after an observed grant, but may
  never toggle or approve the grant.
- Consequential authority likewise needs platform effect metadata. Android's
  explicit Accessibility dismiss action is consequential; a generic clickable
  node is not promoted from its label, app identity, or visual appearance.
- The authority check is a provider-side dispatch invariant, not a semantic-tool
  convention. Semantic nodes, coordinate clicks, visual marks, long presses,
  drags, scrolls, text edits, and key sequences must all present an immutable
  observation-bound node or surface lease before Android receives input.
- After required structured arguments validate, a fresh structurally active
  Android-owned user-step surface returns the same `os_owned_confirmation`
  receipt across every mutating input route before stale-frame, missing-editor,
  grounding-route, or alternate-provider failures can obscure that checkpoint.
  No input is dispatched and the receipt remains proven no effect.
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

A grounding request failure reports request freshness separately from provider
health. `stale_frame`, `stale_target`, `target_not_found`, and other
frame/request outcomes do not claim that the model chain became unavailable;
only a genuinely missing key or unusable grounding provider reports
`unavailable`.

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

- Phone Control writes one bounded two-file JSONL diagnostic journal per
  participating application process under its app-specific external-files
  directory. Separate journals prevent cross-process append and rotation races.
  Writes are asynchronous and best effort: diagnostic failure or backpressure
  can never affect the runtime. The authority bridge initializes only this
  minimal diagnostic writer, never the full application container.
  `mobile/scripts/collect-phone-control-diagnostics.ps1` discovers and collects
  every process journal plus filtered Logcat from one exact device, Android user
  0, and package, then merges records by timestamp. The collector also emits a
  bounded structural timeline tail and summary with an explicit omitted-record
  count; raw files remain evidence, not the primary diagnosis view.
- Journal schema v3 gives every record a stable process role, process-session
  identity, monotonic sequence, event name, and typed structural fields. Bridge
  setup records include service lifecycle plus terminal connect, pair, and
  authority verification outcomes without endpoints, pairing codes, keys,
  tokens, command output, or other authentication material. Turn records carry
  generation and elapsed time without transcript text. Tool dispatch/receipt
  records carry turn, generation, job, elapsed time, capability/provider state,
  observation identity, effect certainty, invalidation, recovery, retry, and
  user-step fields when present.
- Assistant gateway records contain only route, gateway task identity, and the
  coordinator-dispatch request bit. Coordinator open/re-entry records carry the
  structural source acknowledgement. Neither record contains assist extras or
  claims that a requested dispatch became visible.
- Tool dispatch records include only sorted argument field names plus aggregate
  field and UTF-8 byte counts, never argument values. Tool receipts retain
  bounded `failure_class` and `provider_route_error` symbols when present so a
  handler failure can be distinguished from dispatch-plan rejection without
  collecting paths, URLs, text, or other content.
- Visual-grounding receipts may additionally carry bounded stage and timing
  symbols (`grounding_stage`, mapping, location, semantic verification, and
  final pixel-lease milliseconds). These contain no image, target description,
  model response, or other user content and make slow/stale stages independently
  diagnosable.
- The persistent journal accepts structural event summaries only. It preserves
  Unicode but never persists exception messages or stack traces. Call sites
  must not place speech, model text, node text, URLs, paths, clipboard/file/page
  content, command output, keys, tokens, or authentication material in an event.
- Field admission is a typed central allowlist. Unknown names, type mismatches,
  and free-form values are omitted from both the journal and the filtered
  Phone Control console summary. Unknown event names become a content-free
  `diagnostic_event`; call-site formatting alone cannot make data persistent.
  Throwable console summaries contain only the exception class and bounded code
  locations, never the exception message.
- Structured traces carry turn, generation, job, snapshot, surface, capability,
  provider, timestamps, cancellation, receipt, postcondition, and typed-error
  identity. This is the diagnosis source; console prose is only a safe summary.
- Accessibility content churn is counted but does not emit periodic semantic-
  only journal lines. A coalesced invalidation record is emitted only for hard
  lease invalidation, or when a stale/action failure needs the counters as
  evidence. Diagnostic volume must follow actionable state transitions rather
  than ambient UI animation.
- Visual streaming records `screen_capture_route` only when its provider
  changes, including whether the live overlay was mutated. It emits no
  per-frame heartbeat, so diagnostics can distinguish Accessibility,
  projection-only continuity, and visual grounding without creating log or
  rendering churn.
- Projection decode failures emit once at the failure transition and then at a
  bounded repeat cadence. The count resets only after the complete frame,
  metadata, cache, and caller-copy path succeeds. Failure reporting uses a
  pre-close structural image snapshot and can never touch or close the frame
  unsafely.
- Microphone diagnostics record every structural voiced-burst start and end
  without transcript content, plus bounded capture-reopen attempts after a
  platform read failure. This makes a silent/dead input path diagnosable without
  persisting speech.
- When collecting schema-v1 journals, compact legacy periodic invalidation
  summaries before JSON parsing. Preserve aggregate record, hard-invalidation,
  and semantic-invalidation counts in `summary.json`; omit those legacy lines
  from the causal timeline. Schema-v2 records remain unchanged.
- A changed child path inside the same live generation and exact display/window
  may recover only after a bounded, complete traversal finds exactly one node
  with the lease's full structural fingerprint. Ambiguous, incomplete, or
  different-generation recovery is rejected as stale. Platform child lookup
  exceptions become typed evidence and never escape the provider boundary.
- After platform dispatch, a provider-read failure is
  `postcondition_unavailable`; a failed required state check is
  `postcondition_not_verified`. Both preserve
  `effect_may_have_occurred`, invalidate the snapshot, and require a fresh
  observation. Neither may be flattened into `ok`.
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
- Shared Live and visual-grounding model contract:
  `parity-fixtures/phone-control/model-chain.json`
- Shared turn/effect fixture:
  `parity-fixtures/phone-control/turn-contract.json`
- Shared file-mutation fixture:
  `parity-fixtures/phone-control/file-mutation-contract.json`
- Shared native-runtime identity fixture:
  `parity-fixtures/phone-control/native-runtime-contract.json`
- Shared launcher/activation fixture:
  `parity-fixtures/phone-control/activation-flow.json`
- Shared diagnostics and target-recovery fixture:
  `parity-fixtures/phone-control/diagnostics-contract.json`
- Shared Android WebView rendering fixture:
  `parity-fixtures/android-webview-overlays/rendering-contract.json`
- Shared socket lifecycle fixture:
  `parity-fixtures/gemini-live-session/lifecycle.json`
- Windows acceptance suite:
  `tests/computer_control_golden_suite.json`

### Verified Android evidence (2026-07-26)

- The latest completed Full and Play unit runs pass 734 and 723 tests
  respectively; neither run has failures, errors, or skips.
- Clean Full and Play installs each pass their shared instrumentation harness on
  a disposable Android virtual device. Visual grounding requires no
  flavor-specific model asset or native runtime.
- Clean Full and Play debug installs each pass three device-local SGT Bridge
  primitive tests covering binding lifecycle, app-owned non-exportable ADB
  signing keys, and authenticated-loopback secret stripping. The release package
  remains unchanged and no debug/test package remains after cleanup. These probes
  do not claim a real wireless-debugging pairing or authenticated Chrome session.
  The same tests are part of the standard Full/Play harness class set.
- A retained Full debug install on a physical current-API device passes those
  same three bridge primitives, reconnects through its real wireless-debugging
  pairing, accepts only a completed UID-2000 authority receipt, and opens the
  authenticated Chrome CDP tunnel. The production tool stack enumerates the
  existing credentialed-browser targets, binds one exact disposable public
  target, reads its complete DOM text into a local artifact, and verifies that
  target absent after close. The normal release package version and update time,
  original foreground, and user-granted Accessibility state remain unchanged.
- The final Play release AAB passes module ownership plus exact native/model
  byte-count and SHA-256 checks.
- A Full debug build on a physical current-API device repeatedly starts
  whole-display MediaProjection, publishes a first frame, opens microphone
  uplink, verifies the paired SGT Bridge authority, and then stops projection
  while the app process remains alive. The post-fix run contains no projection
  decode failure, closed-image access, activity-only display lookup, or fatal
  exception.
- Production-path probes on real Settings surfaces verify a routine navigation
  postcondition, stale-target rejection with proven no effect, and an OS-owned
  Package Installer confirmation that remains a user step and preserves the
  installed package.
- A visually blind target entered its own process-failure surface before usable
  content existed. Phone Control returned a typed degraded, proven-no-effect
  result. A successful blind-surface visual action, Shizuku, root, and broader
  device variants remain evidence gaps, not claimed passes.

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
- strict named-point, multi-mark, and dual-endpoint parser tests across the
  grounding chain, including malformed output and target-not-visible behavior;
- real-device latency and coordinate accuracy for named clicks, dense mark maps,
  drag endpoints, fresh crosshair verification, and target-local lease failure;
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
7. Keep the first-party local ADB bridge as the recommended non-root route after
   its pairing, reconnect, cancellation, revoke, and real-device lifecycle
   checks; continue the transport threat-model and dependency-audit work.
8. Optional root/device-owner/privileged providers behind the same registry.
9. Shared Gemini named-point, multi-mark, drag-endpoint, crosshair verification,
   and target-local visual-lease grounding.
10. Real-task golden runs, security isolation, performance/thermal testing, and
    one repair rerun per failed acceptance task.

Implementation audit (2026-07-27):

- Stages 1-9 have production paths or an honest platform-conditional result.
  The shared generated catalog, lifecycle, Accessibility/visual providers,
  files/artifacts/memory/research, SGT Bridge, Shizuku, root command route,
  credentialed Chrome CDP, Custom Tabs, and shared visual grounding are wired in
  both flavors.
- An SGT-owned WebView provider becomes ready only for an actual isolated
  SGT-owned browser surface. It is not a substitute for the user's credentialed
  browser. Device-owner and privileged-system providers likewise become ready
  only in their real provisioned deployment.
- The five desktop app-integration management tools remain declared and return a
  typed unavailable result because Android has no corresponding installed
  desktop stdio-MCP catalog or process transport. A future Android integration
  transport must join the same generated catalog; it must not be faked with UI
  clicks or a hand-copied mobile list.
- `click_here` and `point_at` remain typed platform limits because Android
  exposes no universal persistent pointer/hover state. Touch, semantic actions,
  coordinates, visual marks, and target grounding remain fully available.
- Stage 10 and the physical-device matrix are acceptance work, not missing core
  runtime code. An emulator can prove lifecycle, catalog, provider contracts,
  app-owned storage, local tunnel isolation, and visual-grounding contracts. It
  cannot prove OEM behavior, real wireless-debugging/Shizuku/root authority,
  signed-in Chrome CDP, radio/thermal behavior, or hardware-backed user steps.

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
- MediaProjection consent is a required, session-scoped Android deviation and
  cannot be cached as a perpetual grant. Once its live capture session starts,
  the runtime capability snapshot reports `media_projection=ready`; only an
  absent or revoked session reports `needs_user_step`. See Android's
  [media-projection guide](https://developer.android.com/media/grow/media-projection).
- Android 15 may replace private notifications with their public version during
  whole-screen sharing. Protected setup checkpoints follow Android's
  [screen-share protection contract](https://developer.android.com/about/versions/15/behavior-changes-all#screen-share-protection)
  instead of retrying an inaccessible notification action.
- Shizuku ADB startup currently does not survive reboot; root/device-owner/system
  deployments have different lifecycle contracts.
- All other behavior defaults to the Windows contract.
