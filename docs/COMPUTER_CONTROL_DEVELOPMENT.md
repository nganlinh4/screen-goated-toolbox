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
- Prompt text must not enter process arguments, logs, telemetry, discovery, or
  durable storage. `cancel` stops the owning control session; it does not invent
  a continuation or a second runtime.
- Computer Control command-line modes are not a product or test entrance. Test
  the repaired application through this real runtime surface.

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

## Grounding and effects

- Prefer semantic browser/native state, then current-frame vision.
- Visual point, mark, drag, and crosshair decisions use bounded JSON schemas.
  Provider adapters select their strongest catalog-declared structured-output
  transport; local validation remains mandatory and fails closed.
- Coordinates and element IDs are bound to frame, view, document, and surface.
- Re-observe after change or uncertainty. Never reuse stale targets.
- An interrupted mutation with no delivered no-effect receipt blocks later
  mutation/completion until a fresh read reconciles current state.
- Text entry does not authorize Enter, submit, send, publish, buy, or delete.
- Requested routine actions proceed. Confirm only an unrequested irreversible,
  destructive, financial, privacy-sensitive, or external-commitment effect.
- Preserve unrelated state. Exact edits keep path, format, delimiters, and bytes.

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
