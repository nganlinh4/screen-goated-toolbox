# Computer Control Development Contract

Normative for `src/overlay/computer_control`.

## Prime

- The live model owns language meaning, planning, and semantic completion.
- Every normal turn gets the full tool catalog.
- Rust gates structure and effects, never phrases or inferred intent.
- Unknown future tools stay usable by default.
- One user turn produces at most one final answer, then idle.

## Allowed code gates

- one in-flight job and monotonic cancellation;
- stale frame, stale element, wrong surface, and wrong focus rejection;
- required structured fields and input-injection accounting;
- explicit confirmation for consequential external effects;
- resource-bound checks beside durable file or process mutations;
- action receipts and fresh postconditions;
- reconnect, audio ownership, and late-event retirement;
- bounded repeat-failure protection based on typed outcomes.

Do not add language-, phrase-, app-, site-, person-, task-, or incident-specific
permission logic. Do not silently reroute the model's requested tool.

## Completion

- `done` is the model's terminal signal, not a request for another model to judge
  the whole task.
- Accept `done`, release its generation audio once, close the generation, and
  reject later tools or speech until the next user turn.
- If an action generation ends without `done`, release its one response and
  return idle. Never create a synthetic continuation or verification turn.
- Turn-end may enqueue local cleanup such as retiring turn-owned tabs. Cleanup
  produces no model response and cannot reopen the turn.
- A user-visible open persists by default. Disposable browser work must choose
  an explicit turn lifetime; research-owned tabs never escape research cleanup.
- Keep per-action postconditions. They tell the acting model what actually
  happened before it decides whether to continue or finish.
- Independent correctness belongs in the test oracle, not in a production
  completion quorum. A model claim is never the test oracle.

## Startup

- A missing or explicitly rejected Gemini API key does not leave a Computer
  Control orb running. Show the localized error through the shared notification
  badge so hotkey and footer launches have the same visible outcome.
- Classify credentials only from explicit provider evidence. Network, quota,
  model, and generic setup failures must not be mislabeled as key failures.
- Cancellation retains a typed identity through startup and error context.
  Connection deadlines remain failures. Resource-limit status requires an
  explicit provider status/code or HTTP 429; generic error wording cannot
  establish a provider limit or a particular retry interval.
- Initial setup, its bounded retry, and reconnect request the complete normal
  catalog, including provider search. A setup failure cannot silently select a
  reduced catalog or claim a missing capability; report the setup failure.
- Check aggregate MCP integration/tool counts before normalization and the exact
  encoded declarations against setup admission before publishing routes. An
  oversized catalog fails as a whole, with a capacity diagnostic in Downloaded
  Tools and integration status. Keep the diagnostic through failed-session
  teardown; clear it on successful complete-catalog admission or explicit
  integration removal. Never silently trim otherwise representable tools to fit.
- Host and engine provider-field validation share their wire limits. Optional
  connection metadata may be absent or empty; a valid bounded engine response
  must not fail because the host imposes a smaller subfield budget.
  Decoded content preserves Unicode codepoints within its byte limit; effect
  identifiers retain their separate validity checks.
- Reject a provider-data frame before effects without retiring its healthy
  decoder or live session. Local request rejection, an authenticated application
  rejection, and invalid decoded provider data have explicit types. Transport,
  framing, authentication, ABI and unclassified engine faults remain fatal.
  A rejected batch publishes no valid prefix or progress/completion signal.
  Diagnostics contain bounded per-session counters, never frame/error content.
- Cancelled provider call IDs lose any queued response before terminal-drain
  handling. Replacement transports discard every queued response from the old
  connection; an action retains its local ownership until its actual result
  settles.
- Reconnect failure counts reset only after the previous established connection
  lasted at least 30 seconds. Receiving a provider frame does not reset them.
  Attempts wait 250 ms, then double to a four-second cap; stop interrupts the
  wait, and pending work is cancelled before waiting.

## External invocation

- The running desktop app exposes a loopback-only authenticated JSON control
  surface for `launch`, `submit_turn`, `status`, `cancel`, and `stop`.
- Discovery is a current-user file containing the ephemeral endpoint and bearer
  token. The server removes only its own matching discovery record at shutdown.
- A submitted prompt enters the ordinary visible Computer Control runtime and
  receives the same catalog, observations, checkpoints, receipts, and one-final
  lifecycle as speech or orb text input. It never starts a headless controller.
- Keep at most one bounded startup prompt and one in-flight turn. Reject excess,
  blank, oversized, unauthenticated, stale, or malformed requests explicitly.
- Request headers and body share one absolute read deadline; incremental traffic
  cannot extend it. The listener blocks while idle and wakes on owned shutdown.
- Prompt text must not enter process arguments, logs, telemetry, discovery, or
  durable storage. `cancel` stops the owning control session; it does not invent
  a continuation or a second runtime.
- External commands retain their nonpersistent policy through startup and live
  queues. Admission disables automatic diagnostics, trace artifacts and memory
  for the rest of that session; later replies and observations may still contain
  the input. Reconnect context remains in memory. Large tool artifacts remain
  readable in memory within their owning session, and explicitly requested file
  exports still execute. Detached work inherits its owner's policy, and session
  artifact memory is bounded to 32 MiB and 128 entries without eviction. Session
  retirement prevents new diagnostic writes or cache publication. An admitted
  save of immutable ordinary-session memory may finish after teardown.
- Computer Control command-line modes are not a product or test entrance. Test
  the repaired application through this real runtime surface.

## Conversation memory

Automatic conversation memory keeps at most six representative images from
frames successfully sent to the model for confirmed user turns. It never
captures another image on the realtime loop or admits idle ambient frames.
Speech frames remain provisional until a transcript confirms the turn; expired
provisional frames and stale snapshots are excluded. Each saved image carries
its delivered frame/view/surface identity and confirmed turn ID. Image decoding,
disk writes and embedding run on the detached finished-session save worker.

Completed versioned conversation sidecars are the durable source of truth for
transcript, image provenance, metadata and any available embedding. The search
index is derived and rebuildable. Store transactions serialize across threads
and processes at the selected runtime root; network embedding runs outside the
lock, after durable publication, and cannot resurrect a pruned record. Preserve
corrupt bytes before rebuilding from healthy records, propagate read/publication failures,
and keep unsupported record versions intact. Retention uses recoverable retirement
records and deletes only canonical companion images owned by those records.
Report temporarily blocked physical cleanup and retry it on later transactions
without hiding the healthy retained records.
If embedding fails, the conversation remains available through keyword search;
there is no automatic embedding backfill.

## Engine boundary

- The optional x64 engine owns only data-only planning, the complete static tool
  catalog, prompt construction, and provider-frame interpretation.
- The signed host owns credentials and transport, observations and freshness,
  microphone and playback, job identity and cancellation, confirmation and
  checkpoints, every effect, action receipts, postconditions, reconnects, and
  the one-final-response lifecycle.
- The engine receives no unrestricted effect API, raw command channel, trusted
  arbitrary path, provider secret, or independently usable network authority.
- Host and engine communicate over inherited private pipes with a per-launch
  authentication secret, monotonic request IDs, an exact protocol handshake,
  and bounded schemas, frames, and deadlines.
- The host validates the full catalog, dynamic integration declarations,
  privilege/search/voice policy, and every effect-bearing provider event before
  using engine output.
- Engine installation is exact-size and SHA-256 verified. Its lease and locked
  inventory last for the child process lifetime; removal through Downloaded
  Tools becomes pending while a session is active.

## Speech

- Audio belongs to its generation.
- Stream current-generation PCM as it arrives; only the small device startup
  buffer or temporary absence of an output sink may delay playback.
- Tool dispatch and semantic completion outcomes never gate current speech.
- A silent `done` may open exactly one short final-response generation.
- Barge-in cancels only the owned pending job. Dropped generations never play
  later.
- Short, low-confidence local RMS activity remains diagnostic noise unless a
  provider transcript arrives; it must not masquerade as a lost command.
- Local acoustic pauses do not end a collecting transcript. A new local speech
  candidate may open an epoch after a completed response; explicit provider
  interruption or cancellation of the owned job can arm a new input during
  active work. Playback of a completed response cannot suppress the next
  candidate. Late
  ASR without new input evidence updates its existing turn without reviving it.
  Frame requests may refresh on resumed speech without splitting transcript,
  goal or history ownership. Only a real transcript commits the candidate.
  Local activity alone never supersedes an in-flight action; provider-owned
  cancellation and confirmed new transcripts retain their job-identity checks.
- Each new input epoch requests a frame whose capture starts after that epoch.
  Retire the old source binding immediately, wake the capture worker, and keep
  the request pending through transcript arrival and playback until a successful
  fresh delivery. Never wait for capture on the microphone/socket loop. Ordinary
  cached deliveries must also pass an age bound; an in-flight older capture or
  failed send cannot fulfill a fresh-frame request. Unconfirmed noise expires,
  and completed turns retire outstanding requests rather than resuming capture.
  An older action's receipt still settles normally during new input, but its
  screenshot cannot replace that input's view; request a new capture after the
  action settles.

## Grounding and effects

- Prefer semantic browser/native state, then current-frame vision.
- Visual point, mark, drag, and crosshair decisions use bounded JSON schemas.
  Provider adapters select their strongest catalog-declared structured-output
  transport; local validation remains mandatory and fails closed.
- Coordinates and element IDs are bound to frame, view, document, and surface.
- Encoded images, view identity, coordinate transforms and target signatures
  use the same clipped screen rectangle. A delivered frame owns its rectangle;
  action state cannot reconstruct it from a later window-position lookup.
  Visual grounding retains that rectangle through verification and dispatch,
  and a changed image region requires a fresh observation.
- Native indexed actions, visual targets, coarse-click guards, and text-value
  observation use one control-type capability table. Containers do not become
  action targets solely because they surround interactive descendants.
- Re-observe after change or uncertainty. Never reuse stale targets.
- Native text observations use bounded previews. Preview availability is separate
  from editing capability and never proves an exact field value or form validity.
  Exact fill verification reads the targeted control after dispatch. Required
  fields with unavailable values remain unobserved, rather than empty or filled.
  Background control-grid snapshots read labels and geometry without text/state
  patterns.
- Repeated-action visual advice requires observed unchanged accessibility state
  on the same surface and a matching unchanged pre/post visual sample. Failed or
  empty accessibility observations and missing visual samples are unknown;
  visible progress or unknown evidence resets the repeated-no-progress history.
- An interrupted mutation with no delivered no-effect receipt blocks later
  mutation/completion until a fresh read reconciles current state.
- Effect evidence belongs to the host receipt, never arbitrary nested tool,
  page or provider data. Host wrappers publish their computed certainty at the
  outer boundary. Preserve opaque payload fields without interpreting them as
  execution proof. A batch retains uncertainty from every attempted step.
- Text entry does not authorize Enter, submit, send, publish, buy, or delete.
- Requested routine actions proceed. Confirm only an unrequested irreversible,
  destructive, financial, privacy-sensitive, or external-commitment effect.
- Preserve unrelated state. Exact edits keep path, format, delimiters, and bytes.
- After a rejected exact-file transaction, alternate mutating capabilities need
  a fresh independent effect assessment against the active resource boundary.
  Assess the selected session declaration, exact arguments and observed context;
  path occurrence or absence in arbitrary text cannot establish permission.
  Dedicated exact text transactions retain their own preflights. Artifact
  replacement cannot bypass a rejected text transaction's format/structure checks.
- Resource evidence distinguishes a destination entry from its current file
  object. Capture identity before dispatch; hard-link aliases share failure
  evidence, while atomic replacement retires only the selected entry. Current
  receipt bytes must reconcile under a bounded stable read lease before evidence
  is cleared. Unknown identity stays uncredited. Turn retirement releases all
  retained ownership, which can otherwise keep Windows directories busy.

## MCP request ownership

MCP action cancellation carries the shared owning job flag through encoding,
writer admission and reply waiting. The writer checks cancellation and the
request deadline before claiming a queued frame. An unclaimed frame can be
cancelled with proof of no effect;
once writing starts, an interrupted frame retires that owned transport. After a
complete write, request cancellation is advisory and cannot prove rollback.
Ignore retired reply IDs, preserve uncertainty, and keep healthy peers usable.
The writer retains the child and pipe until its actual write settles; do not
detach a mutating RPC through a generic cancellable observation worker.
See the [MCP cancellation contract](https://modelcontextprotocol.io/specification/2025-06-18/basic/utilities/cancellation).

## Program lookup

Exact-process mode keeps program selection separate from the child's working
directory. A bare program uses configured search directories and native system
lookup; selecting a working directory does not add it to that search. A local
program uses an explicit relative or absolute path. Explicit PATH entries retain
their meaning, including a deliberately configured working-directory entry.
Supported executable shims and extension order remain available. Assessment and
execution use the same resolved process description.
Explicit relative programs remain anchored to the selected cwd even when absent;
configured relative search entries resolve before the child's cwd is applied.
Windows drive-relative program paths require an unambiguous relative or absolute
path instead of guessing per-drive working-directory state.

## Diagnostics

- Text artifacts belong to their exact session and share a 32 MiB content and
  metadata budget and a 128-entry limit. Import and export read-back use bounded
  regular-file reads. Ordinary cache files are exclusively created, read through
  their owned handles and removed when the last admitted lease closes; session
  retirement removes their cache entries. Exported user files are separate.
  Informational source URLs omit credentials, queries, fragments and opaque
  payloads. Unowned historical cache files are preserved.

- Global diagnostics contain only static event/component names, static message
  templates, correlation IDs, counts and value shapes. Rendered diagnostic text
  and detailed event fields belong only to the session trace. Runtime field
  names or values never become globally safe merely because of their key.
- The session trace is append-only: it keeps its first 64 MiB, then writes one
  `telemetry_session_capped` marker. A completed receipt that the model never
  received (cancelled, superseded or stale job) is kept there as
  `action_result_dropped_receipt`, within the tool-response size budget.

## Tests

- Test capability and lifecycle invariants, not utterance dictionaries.
- Use natural goal-level tasks on real applications and controlled disposable
  data. Do not spoonfeed tool names, selectors, or coordinates.
- Use `tests/COMPUTER_CONTROL_GOLDEN_SUITE.md` for long-run evaluation. Its
  independent oracle decides task correctness.
- A live run gets a new evidence directory and an isolated absolute
  `SGT_RUNTIME_STATE_ROOT`. Keep `LOCALAPPDATA` unchanged.
- Require exactly one final response, settled idle, no post-completion effect,
  and no unrelated state change.
- Scripted idle starts only after queued turn cleanup is acknowledged.
- Keep every individual live execution bounded, but continue the
  diagnose-repair-rerun cycle until the acceptance task succeeds. Each failure
  gets fresh evidence, competing-cause review, a focused repair, focused
  verification, and a new evidence directory before rerunning.
- Never report acceptance complete from unit, compile, lint, or dry-run results
  alone. Require the repaired build to complete the real task and validate its
  committed artifact.
- Stop only at a safety boundary or a genuinely external blocker that cannot be
  resolved within the task's authority. Preserve the evidence and report
  `blocked`; never turn a repeated failure into a pass or abandon the required
  rerun.
- Keep benchmark names and artifacts out of production prompts and logic.

## Verify

Run focused tests for the changed invariant, then:

```powershell
cargo fmt -- --check
cargo test
cargo clippy --all-targets -- -D warnings
git diff --check
```

Use `scripts/validate-windows-targets.ps1` for x64 target-sensitive changes. Never
run `cargo build --release` during development.
