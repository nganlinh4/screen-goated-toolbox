# Image to SVG Parity

Storage and destination availability follow the shared
[Creation storage contract](creation-storage.md).

The creation backend is external. This contract covers the frontend, the host
client/IPC side, and the delivery steps; backend internals are out of scope.

## Restoration Status

- The mini app is an active optional creation capability on Windows, Android
  Full, and Android Play. Each platform exposes the same product contract and
  installs the checksum-pinned creation runtime before first use.
- Image-to-SVG is delivered alongside Image to 3D. The two capabilities remain
  independently gated by their shared release-availability fixtures.

## Canonical Source

- Windows launcher and host: [image_to_svg](../../src/overlay/image_to_svg)
- Windows creation UI: [image-to-svg-ui](../../image-to-svg-ui/src)
- Shared result history: [generation_history.rs](../../src/overlay/generation_history.rs)
- Shared fixture: [state-contract.json](../../parity-fixtures/image-to-svg/state-contract.json)
- Android creation UI:
  [creation](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation)

## Product Contract

- Release availability is owned by the shared product fixture. The visible
  launcher opens the creation surface, then starts readiness after first paint.
  First use repairs the selected optional runtime before accepting work.
- One picker or drop may add multiple images as a batch.
- Every SVG job has exactly one source image. A multi-image batch creates
  independent one-image sessions; references are never combined into one job.
  One picker/drop action accepts up to 100 images; each must be a PNG, JPEG, or
  WebP no larger than 25 MiB, 32,768 pixels on either axis, or 64 million
  decoded pixels.
- The primary action submits only the selected session. Other sessions from the
  same import remain drafts; parallel work requires an explicit submission for
  each session.
- Every explicit submission creates a fresh dispatch identifier and a fresh
  generation. Restart replay is recognized as the same request only when that
  exact dispatch matches; equal image bytes and settings never collapse two
  user actions.
- Once the selected session is queued or running, its primary creation action
  is disabled and visually inactive until that job becomes terminal.
- Distinct explicit submissions from sessions that are eligible to submit are
  captured as distinct jobs. Start-response order cannot collapse requests or
  steal a newer user selection.
- A done, failed, or cancelled session may be submitted again. The retry keeps
  the previous published artifact/history entry and creates a fresh job only
  for that selected session.
- A batch freezes Simple or Detail and its transparent-background choice before
  entering the queue. A later batch may use different choices.
- Simple and Detail are stable product choices. Their implementation mapping
  and capacity policy remain outside the public contract.
- Transparent background is a compact three-state product choice: Auto lets the
  creation capability decide, On requests transparent output, and Off preserves
  an opaque background. Off is the compatibility default. Every explicit
  submission freezes the selected choice through queueing, recovery, delivery,
  history, and rerun.
- Recent sessions are presented newest-first on every surface, including newly
  imported, queued, processing, and saved-result rows. Presentation sorting
  never changes pending dispatch order.
- Deleting one saved result is immediate. Delete all requires confirmation,
  then removes every saved result for this mini app without affecting the
  other creation tools.
- At most two jobs run simultaneously. SVG, 3D, and image jobs have independent
  concurrency limits.
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
- Progress preserves `draft`, `queued`, `preparing`, `generating`,
  `finalizing`, `cancelling`, `done`, `failed`, and `cancelled`.
- The selected source preview, public stages, measured ETA, and real SVG are
  the generation visuals.
- Queue and history rows use the shared image icon and never decode artwork.
  Only the selected canvas source is streamed through the platform image
  decoder, without a synchronous decode/resize/re-encode step in the
  interaction path. Original image bytes are not retained in JavaScript.
  Session creation and queue interaction stay independent of preview decoding.
- Android retains a persistable user-owned source handle when available and
  materializes full bytes only while the job is queued, running, or recoverable.
  History keeps a bounded presentation derivative, not a private full-size
  duplicate. Job copies are removed after the delivery/history transaction
  commits. If source access is later revoked, the result remains available and
  a new submission asks the user to select the source again; a thumbnail is
  never substituted as generation input.
- Completion renders the complete safe static SVG presentation at its intrinsic
  ratio, including groups, symbols/use, text, embedded bounded raster images,
  gradients, patterns, clipping, and masks. Filter elements, filter primitives,
  and CSS filter properties are rejected because unbounded SVG filter graphs and
  regions are not bounded by the document's structural budgets. Scripts, event
  handlers, `foreignObject`, external/network/file references, navigation,
  DTD/entities, XML processing instructions, CSS animation/transition, and
  other active CSS are also rejected before publication.
- The root and every element use the canonical SVG namespace. Only the
  canonical default SVG declaration and optional canonical XLink declaration
  are accepted; other qualified attributes or unbound prefixes are rejected.
  Embedded PNG/JPEG data URIs are valid only on an image element's `href` or
  `xlink:href`. When both are present, unqualified `href` has precedence.
- Untrusted SVG is isolated from the product UI and host capabilities on both
  platforms. It cannot navigate, open a window, access the network or local
  files, or invoke product IPC.
- Unsupported editing semantics remain fully rendered and are explicitly
  non-editable rather than silently dropped. The static presentation does not
  run per-path entrance animation.
- A completed result first opens as the isolated full-fidelity static preview.
  Selecting it does not transfer or parse the editable document in the product
  surface. Loading, validating, and building the editable geometry and hit-test
  tree requires an explicit Edit paths action.
- Viewer controls include fit, zoom, pan, background switching, and the explicit
  Edit paths action. After editing is activated they also include path
  selection, fill/stroke editing, undo, redo, shape deletion, and saving edits
  to the real SVG.
- An edit is validated before publication. Saving never truncates the previous
  result on failure, and the history ownership proof matches the saved bytes.
  Android publishes edits as a new result in the original destination because
  document providers do not guarantee atomic in-place replacement; the old
  user-owned result remains available. A failed save retains the editor draft.
  Managed originals retained for these copies remain visible and explicitly
  removable even after ordinary history retention expires; they are labeled as
  originals and do not count toward the ordinary history limit. If retention
  state is unavailable, protect the files and leave unrelated history usable.
- A stationary primary-pointer press on editable geometry selects that exact
  geometry in one interaction. Pointer capture begins only after the pan
  threshold, so it cannot retarget a click to the artboard and clear the
  intended selection. A completed pan changes only the viewport.
- SVG parsing and editing are bounded by byte, decoded-raster, node, attribute,
  and undo-memory budgets. Undo stores bounded deltas/checkpoints rather than 50
  complete document copies. Documents above 2 MiB or 5,000 editable geometry
  elements still render their complete safe static presentation, but editing is
  disabled so the host never constructs an unbounded duplicate hit-test tree.
  The presentation ceiling is 12 MiB, 50,000 elements, and 250,000 attributes.
  Nesting is limited to 128 elements. Geometry is limited to 250,000 path
  commands and one million finite numeric tokens whose absolute values do not
  exceed 10,000,000. Local render references in `href`, `xlink:href`, and every
  `url(#identifier)` occurrence are limited to 100,000 occurrence edges and 64
  levels, must be acyclic, and use identifiers no longer than 512 UTF-8 bytes.
  Duplicate identifiers, encoded fragment aliases, and stylesheet URL
  indirection are rejected; local resource ownership must be explicit on the
  referencing element.
  Embedded rasters are static PNG or JPEG only; each is at most 2,800,000
  encoded characters and 16 million decoded pixels, with at most 32 million
  decoded embedded-raster pixels across the complete document. The total
  charges every rendered raster occurrence, including repeated references to
  identical embedded bytes; implementations may cache validation but not cost.
  Local render-reference expansion preserves every occurrence, including
  repeated pattern, gradient, mask, clip, marker, and symbol references. The
  document root and every identifier-owned subtree carry direct element and
  raster cost; recursive expansion must remain acyclic and within the same
  100,000 reference-occurrence, 50,000 expanded-element, and 32-million
  expanded-raster occurrence ceilings.
- History lists only existing outputs. Rename and delete operate on the
  published file.
- The default destination is the user's Downloads folder. Default and
  explicitly selected destinations are user-owned, so SGT never removes their
  result files automatically. Legacy results and jobs accepted with the former
  app-managed destination remain recoverable and keep their existing retention
  behavior.
- Output delivery is a durable transaction keyed by the dispatch identifier:
  validated result, prepared publication receipt, published location, history
  commit, then recovery-intent removal. Restart reconciliation resumes that
  transaction without repeating generation or publishing a second copy. A
  published file with uncertain ownership is retained for the user.
  Delivery retry is bounded. Before anything is published, a permanent
  destination error (revoked folder access, missing destination or source) or
  a persisted attempt budget ends the job as failed and releases its
  reservation, staging output, and inputs; once a file is published, delivery
  keeps resuming until history records it.
- History rename is transactional. A failed index commit rolls the file move
  back, and restart reconciliation repairs an interrupted rename without
  orphaning the result.
- History uses the shared user setting: 50 entries by default, configurable
  from 10 through 200 per creation tool, under a shared 4 GiB managed-artifact
  budget. The newest result per tool is protected; older rows are pruned by age
  until both limits hold. Pruning performs the same path-confined artifact
  cleanup as explicit deletion and never touches live, recovery-owned,
  user-exported, or externally modified files. History stores the committed
  size and digest so cleanup deletes only the exact bytes it originally
  published.
- Managed admission also reserves at least 1 GiB of free volume space and the
  maximum bytes for the new result. Reclaimable data is pruned before dispatch;
  if live or recoverable data prevents that reserve, the request is rejected
  before generation starts.
- Legacy history rows without committed ownership proof remain user-owned.
  They are not hashed during startup and are never deleted automatically.
  Storage pruning runs before new import admission so reclaimable managed bytes
  cannot create a permanent full-storage dead end.

## Shared Visual System

- Windows uses the shared creation-app title bar, queue rail, stage, controls,
  dialogs, focus treatment, typography, icon system, light/dark tokens, and
  reduced-motion behavior.
- Filled Material Symbols Rounded come from the shared SGT icon catalog; the
  mini app does not carry a separate hand-authored icon set.
- Android uses the shared native Material 3 Expressive creation shell and
  preserves the same settings order, progress states, preview math, result
  controls, and history behavior.
- An unchanged status refresh or thumbnail completion preserves existing interactive
  queue targets.
  A status refresh never replaces a hovered selection target, focused input, or active
  IME composition. The queue owns its overflow, and the primary action remains
  reachable regardless of queue length.

## Public Boundary

- Public source, fixtures, tests, diagnostics, history, and UI describe only
  product settings, jobs, stages, artifacts, and stable IPC state.
- Public requests expose Simple/Detail, the three stable transparent-background
  states, and advertised capabilities only.
- Implementation details and raw errors remain outside this repository's
  contracts, diagnostics, history, and presentation.
- Every inbound event is normalized before storage, logging, or display.

## Failure And Recovery

- A failed job remains failed and cannot publish a stale or previous result.
- Submission and recovery are bounded; an accepted request is never submitted
  twice.
- Execution preparation, capacity handling, and worker retirement are backend
  internals. Recovery stays scoped to the affected product capability and never
  exposes implementation details through UI, history, public diagnostics, or
  the shared fixture.
- Restart recovery binds to the same dispatch rather than submitting it again.
- Cancellation wins over late success and stale completions cannot affect newer
  executions.
- Input validation belongs to its captured submission. Closing or cancelling that
  submission retires validation and its dialog before it can start work. Dismissing
  the dialog submits nothing and leaves published results intact. A late start
  acknowledgement is reconciled against that same submission and cancelled when
  it was retired;it cannot replace another session's state.
- Cancellation remains pending while the host reports `cancelling`. The surface
  displays the authoritative cancellation outcome. A result committed before the
  cancellation decision remains done;an unavailable decision is not presented as
  successful cancellation.
- A creation surface may contain multiple independent sessions. Closing it
  cancels its work, releases its owned execution resources, destroys the
  product surface, and prevents late completion from publishing.

## Verification

- Shared fixture: `parity-fixtures/image-to-svg/state-contract.json`
- Windows tests verify queue behavior, stage mapping, SVG validation, editor
  operations, cancellation, and history.
- Android Full and Play tests read the same product fixture and verify session
  isolation, concurrency, the preview contract, SVG result behavior, and
  history.
- Real UI acceptance submits one Simple job on Windows, Android Full, and
  Android Play. It selects the source and setting through the product surface,
  waits for a terminal state, and validates the committed SVG.
- A failed real UI case retains bounded terminal diagnostics and reruns in a
  fresh evidence directory after repair; a backend-side cause is handed to the
  backend repository. Acceptance is incomplete until the repaired build completes the
  real job and the committed SVG validates; repeated repair failures never
  cancel the required rerun.

## Platform Deviations
- Android submission validation is bound to the selected immutable draft snapshot. A changed selection or configuration invalidates the result. The validation dialog preserves prior artifacts and uses the same choose/retry outcomes as Windows.

- Both platforms check input validity before submission and show an accessible
  Choose another image dialog for rejected input; unavailable checks offer
  Retry. The maximum submission size is 4,490,000 bytes. Android also validates
  the job-owned image before queueing. Dismissal submits nothing and preserves
  existing generated results.

- Both platforms default to the user's Downloads folder. Windows uses the
  system Downloads directory; Android publishes through MediaStore Downloads.
  Windows may use a selected filesystem folder and Android may use a persisted
  Storage Access Framework directory instead.
- Android and Windows may use different isolated rendering primitives, but both
  consume the same sanitized safe-SVG contract and render the same supported
  static presentation.
- Platform delivery differs, but product settings, progress, cancellation,
  artifact, editing, and history behavior remains identical.
