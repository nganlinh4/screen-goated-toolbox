# Image Creation And Editing Parity

## Restoration Status

- The mini app is temporarily hidden on Windows, Android Full, and Android Play.
  Each platform preserves its compiled host, frontend assets, retained state,
  and checksum-pinned runtime capability so a later compatible provider can
  restore the launcher without reconstructing or re-archiving the feature.
- The release gate removes the launcher and rejects direct job admission. It
  does not start the surface or readiness work while hidden.
- Image creation/editing is independently queued from Image to 3D and Image to
  SVG while all three capabilities share one immutable runtime release.

## Canonical Source

- Product contract:
  [state-contract.json](../../parity-fixtures/image-creation-editing/state-contract.json)
- Windows launcher and host: [image_creator](../../src/overlay/image_creator)
- Windows creation UI: [image-creator-ui](../../image-creator-ui/src)
- Shared creation experience: [image-to-svg-ui](../../image-to-svg-ui/src)
- Shared result history: [generation_history.rs](../../src/overlay/generation_history.rs)
- Android creation UI:
  [creation](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation)

## Product Contract

- The app is named Create/edit image / 이미지 생성/편집 / Tạo/edit ảnh.
- Release availability is owned by the shared product fixture. While hidden,
  no launcher is exposed and neither the creation surface nor readiness starts.
  The preserved implementation resumes the normal first-paint and optional
  runtime-repair contract when release availability is restored.
- Its stable tool identifier is `image`, its operation is `create_image`, and
  its public job prefix is `image_`. References are optional and do not change
  the operation.
- The plus action creates one empty image session immediately. A session accepts
  a nonblank instruction with zero, one, or up to 20 ordered reference images.
- Picking or dropping several references adds them to the current configurable
  session. It does not create one job per reference.
- Submission freezes the session's ordered reference list, instruction, and
  output destination as exactly one queued job. Multiple sessions may be queued
  and run independently.
- Every explicit submission assigns fresh job and dispatch identifiers and
  creates a new image, even when the instruction and references are
  byte-identical to a previous job. Restart replay is recognized as the same
  request only when the dispatch identifier matches.
- The primary action submits only the selected session. No action on one
  session may submit another session implicitly.
- At most two image jobs run simultaneously. Image, 3D, and SVG jobs have
  independent concurrency limits.
- Runtime preparation is implementation-private. The public host observes only
  product job states, enforces the two-job limit, keeps capacity isolated by
  creation tool, and never exposes implementation identities, account state,
  or preparation mechanics through UI, diagnostics, fixtures, or IPC.
- Opening the surface paints the product UI before requesting readiness work.
  Idle surfaces do not poll jobs, history, or readiness. Job status and
  estimated-progress refreshes run only while accepted or recovered work is
  active; history refreshes on open, focus, history mutation, and terminal
  transitions.
- An accepted job has a two-hour whole-job watchdog. Silence and broken execution
  connections may fail earlier. Inbound elapsed and estimated-duration values
  are bounded to the same two-hour ceiling before storage or presentation.
- Public progress uses only `queued`, `preparing`, `uploading`, `generating`,
  `finalizing`, `done`, `failed`, and `cancelled`.
- `uploading` and reference-upload copy are valid only when the frozen request
  contains at least one reference. A text-only request remains `preparing`
  until generation starts.
- Reference delivery first requires the visible semantic upload surface, then
  selects one enabled, connected image input by composer ownership, active
  surface, accessible name, stable identity, and accepted media type. A
  visually hidden implementation input remains eligible for native chooser
  activation; disconnected, disabled, semantically unowned, or equally ranked
  inputs fail closed. A missed local chooser activation may be retried within
  one bounded attempt window without navigating or submitting.
- Post-upload retention remains fail-closed: a populated browser file input is
  local chooser evidence, not proof that the creation surface retained the
  reference. The bounded asynchronous retention wait is independent from the
  ordinary control-discovery window. While waiting for the corresponding visible
  reference control, the flow reconciles safe informational dialogs that can
  appear on first use. Submission requires visible retention of every requested
  reference, including multiplicity when filenames repeat, and the aggregate
  retention window remains bounded within the whole-job deadline.
- Busy progress combines the runtime's measured duration estimate and reported
  ratio with the same monotonic elapsed-time curve used by the 3D and SVG apps.
  It refreshes between visible status updates, shows localized remaining-time copy, and
  never exceeds 94% before terminal success.
- Cancellation is monotonic. A late event cannot revive a cancelled job or
  publish an artifact for it.
- Recovery never duplicates an accepted request. A user retry creates a new job
  without mutating the prior result or history row.
- A recovery handoff publishes execution loss only after the prior worker has
  finished cleanup. The same dispatch cannot re-enter a worker instance that is
  still retiring its previous execution state.
- Success requires a nonempty PNG that decodes to positive bounded dimensions.
  Every accepted result is normalized to PNG. The artifact is committed
  atomically before success is emitted. Published PNGs are at most 64 MiB,
  32,768 pixels on either axis, and 64 million decoded pixels.
- Durable history stores the ordered reference list, instruction, result
  metadata, and completion time. Missing outputs are hidden; rename and delete
  operate on the published file.
- Recent sessions are presented newest-first on every surface, including new
  drafts, uploads, processing jobs, and saved results. Draft and active-job
  execution order is independent from this presentation order.
- Deleting one saved result is immediate. Delete all requires confirmation,
  then removes every saved result for this mini app without affecting the
  other creation tools.
- The default destination is the app-managed creation library, so its exact
  unchanged PNGs are reclaimed with history retention just like SGT History
  media and Recorder projects. Choosing an external folder is an explicit
  export; that copy is user-owned and is never removed automatically.
- Output delivery is a durable transaction keyed by the dispatch identifier:
  validated result, prepared publication receipt, published location, history
  commit, then recovery-intent removal. Restart reconciliation resumes that
  transaction without repeating generation or publishing a second copy. A
  published file with uncertain ownership is retained for the user.
- History rename is transactional. A failed index commit rolls the file move
  back, and restart reconciliation repairs an interrupted rename without
  orphaning the result.
- History uses the shared user setting: 50 entries by default, configurable
  from 10 through 200 per creation tool, under a shared 4 GiB managed-artifact
  budget. The newest result per tool is protected; older rows are pruned by age
  until both limits hold. Pruning removes the corresponding app-owned result
  and derivatives with the same path-confined behavior as explicit deletion;
  live, accepted-recovery, exported, and externally modified files are
  protected. The committed size and digest prove file ownership before
  automatic deletion.
- Managed admission also reserves at least 1 GiB of free volume space plus the
  maximum result and reference-snapshot bytes for the new job. Reclaimable data
  is pruned before dispatch; if live or recoverable data prevents that reserve,
  the request is rejected before generation starts.
- Legacy history rows without committed ownership proof remain user-owned.
  They are not hashed during startup and are never deleted automatically.

## Shared Visual System

- Windows uses the same title bar, queue rail, stage, controls rail, status
  strip, dialogs, focus treatment, light/dark tokens, and reduced-motion rules
  as the Image to SVG and Image to 3D mini apps.
- Status refreshes preserve the instruction field's value, selection, focus,
  and active IME composition.
- Typography uses the shared locally served Google Sans Flex variable font with
  its rounded axis. No app-specific display face is introduced.
- Icons use filled Material Symbols Rounded through the established Windows and
  Android asset conventions and shared SGT icon catalog. The mini app does not
  carry a separate hand-authored icon set.
- Android uses the existing native Material 3 Expressive creation components,
  shared typography, shared shapes, and adaptive result layout.
- The image stage has no white paper layer. Empty space remains transparent over
  the shared theme surface, and a single reference preserves its intrinsic
  aspect ratio instead of being forced into a fixed canvas ratio.
- Queue, history, and reference-list rows never decode artwork. Only selected
  canvas images are streamed through the platform image decoder, with no
  synchronous decode/resize/re-encode step in the interaction path. Original
  image bytes are not retained in JavaScript or duplicated into a persistent
  preview cache. Preview loading is nonblocking and the session action remains
  available while the selected canvas settles.
- Reference ingestion is streaming/path-based and bounded per file, per decoded
  image, and per request. It never constructs repeated Base64/string copies of
  all references in JVM and JavaScript memory.
- Android retains persistable user-owned reference handles when available and
  materializes full bytes only while a job is queued, running, or recoverable.
  History keeps bounded presentation derivatives rather than private full-size
  reference copies. Job copies are removed after the delivery/history
  transaction commits. If access is later revoked, the existing result remains
  available and a new submission asks the user to select missing references
  again; thumbnails are never used as generation input.
- An unchanged status refresh or thumbnail completion preserves existing interactive
  queue targets.
  Pointer hover, focus, active IME composition, and the first selection click
  cannot be invalidated by background reconciliation. A pointer sequence is
  atomic with respect to status refreshes, so background state cannot replace a button
  between pointer-down and click.
- Every explicit press is captured synchronously before asynchronous submission
  begins. Rapid presses on the selected session create distinct jobs and
  dispatch identifiers; response order cannot collapse them or move selection
  away from a session the user chose afterward.
- The image status strip uses the shared estimated-progress presentation from
  the 3D and SVG apps: measured estimate when available, monotonic time-based
  interpolation, localized ETA, and 100% only after success.
- Queue, history, and reference-list rows use shared icons and filenames rather
  than decoding artwork. Only the selected canvas presents raster previews,
  with a strict maximum of two decoded previews for a one-reference
  before/after comparison. A multi-reference draft previews its first ordered
  reference with the total count; a multi-reference result presents the output
  with the reference count while the complete ordered filenames remain in the
  settings list.

## Public Boundary

- Public source, fixtures, tests, diagnostics, history, and UI describe only
  product requests, product progress, artifacts, and stable IPC state.
- Public code carries stable product capabilities only.
- Implementation details and raw errors are excluded from this repository's
  contracts, diagnostics, history, and presentation.
- Inbound runtime events are normalized to the public stages and feature-level
  copy before they are stored, logged, or shown.

## Failure And Recovery

- A malformed request, invalid supplied reference, invalid artifact, cancelled
  job, stale callback, or exhausted bounded recovery fails closed. An empty
  reference list is valid for image creation.
- Failures remain attached to the exact job that produced them.
- A creation surface may contain multiple independent sessions. Closing it
  cancels its work, releases its owned execution resources, destroys the
  product surface, and prevents late publication.
- Output extraction may retry only for the already-created result; it does not
  repeat the user's creation request.

## Verification

- Shared fixture:
  `parity-fixtures/image-creation-editing/state-contract.json`
- Windows tests cover request routing, queue isolation, public event
  normalization, cancellation, artifact validation, and history.
- Android Full and Play tests read the same product fixture and verify
  independent sessions with a two-job concurrency ceiling.
- Windows, Android Full, and Android Play entry tests read the shared release
  availability flag and verify that the launcher is absent, direct admission
  is rejected, and neither the surface nor readiness starts.
- Real generation acceptance is excluded while the release gate is disabled.
  The retained text-only and one-reference cases remain available for a future
  restoration qualification.
- Prompt entry is accepted by the surface's editor state before submission;
  matching rendered text alone is not sufficient acceptance evidence. If the
  sole semantic editor is replaced after activation but before any prompt is
  committed, the flow reacquires the sole eligible replacement and still proves
  the accepted editor state.
- Reference entry is accepted only after the surface exposes its retained
  reference control. File-input population alone is insufficient, and safe
  first-use dialogs are reconciled during a bounded wait independent from
  ordinary control discovery. Every requested reference remains visibly retained
  before submission, including repeated filenames, and their aggregate wait is
  bounded below the whole-job deadline.
- A failed real UI case retains bounded terminal diagnostics, triggers private
  execution-readiness investigation and repair, then reruns in a fresh evidence
  directory. Acceptance is incomplete until the repaired build completes the
  real job and the committed image validates; repeated repair failures never
  cancel the required rerun.
- Preparation advances only after the consequential transition is visibly
  complete. Intermediate navigation is not readiness proof, and the transition
  uses the same real control interaction as the canonical Windows path.
- A transient preparation-delivery failure retries once with fresh isolated
  capacity. It never repeats an accepted image request, and exhaustion fails
  through the normal bounded preparation path.
- Exhausted preparation fails every job waiting on that tool exactly once and
  does not automatically start another preparation cycle. A later explicit
  submission may start a fresh bounded cycle.
- Frontend validation covers the shared font and icon system, public
  vocabulary, light/dark rendering, and focused Vietnamese IME input across
  repeated status updates.

## Platform Deviations

- Both platforms default to the bounded app-managed creation library. Windows
  may export to a selected filesystem folder; Android may export through
  MediaStore or a persisted Storage Access Framework directory.
- A one-reference edit uses the before/after comparison. A text-only creation
  presents the result alone. A multi-reference edit presents the result and the
  reference count. The settings list preserves the complete ordered filenames
  without eagerly decoding every reference.
- Platform delivery differs, but request, progress, cancellation, artifact, and
  history behavior remains identical.
