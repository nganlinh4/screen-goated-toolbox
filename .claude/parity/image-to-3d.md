# Image to 3D Parity

## Canonical Source

- Windows launcher and host:
  [three_d_generator](../../src/overlay/three_d_generator)
- Windows creation UI: [3d-generator-ui](../../3d-generator-ui/src)
- Shared result history: [generation_history.rs](../../src/overlay/generation_history.rs)
- Shared fixture: [state-contract.json](../../parity-fixtures/image-to-3d/state-contract.json)
- Progress fixture: [progress-contract.json](../../parity-fixtures/image-to-3d/progress-contract.json)
- Android creation UI:
  [creation](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation)

## Product Contract

- Refinement controls use the app's typography and compact choice groups.
  Separation offers Simple, Balanced, and Detailed; these describe desired
  granularity rather than guaranteed part counts. Initial topology selection
  and post-generation topology conversion are distinct operations. Rigging
  is advertised only for a compatible textured revision. A displayed resource
  balance alone does not establish an action's entitlement.
- Readiness describes whether work can start, independently from the selected
  artifact's Ready label. Available reusable capacity counts as ready; active
  work or preparation without an available slot is preparing, not a failure.

- Windows validates the source again before submission. Fast mode requires both
  sides to be at least 32 pixels and a file no larger than 20,000,000 bytes.
  These mode-specific limits do not constrain Quality mode. General source
  format, memory and file limits still apply. Invalid sources show a localized
  modal explanation with Choose another image before starting generation.
  Unavailable validation instead offers Retry and does not blame the source.
  Escape/Close dismisses without submission; dialogs preserve the existing
  results and use the native modal focus boundary. Shared boundary cases live in
  `parity-fixtures/image-to-3d/input-contract.json`. Android has not yet ported
  this preflight and must not be claimed as verified against this contract.

- The shipped creation runtime advertises and delivers `image_to_3d`,
  `image_to_svg`, and `image_creator`. Windows and Android expose only the
  capabilities enabled by their shared release-availability fixtures;
  `image_creator` is currently delivered but hidden and unavailable for jobs.
- Windows, Android Full, and Android Play consume one tracked, immutable
  delivery contract. Every artifact has an exact URL, version, size, and
  SHA-256; missing or invalid contract data fails the build. Local runtime
  outputs and environment overrides are never accepted as product delivery.

- The app is named Image to 3D / 이미지를 3D로 / Ảnh sang 3D.
- The generation-mode selector exposes Fast and Quality on every platform.
  Fast supports 100–15,000 polygons and does not offer automatic separation.
  Quality supports 500–20,000 polygons and offers optional automatic
  separation. The selected product mode is preserved through submission,
  recovery, history, and result metadata.

- The runtime submits the frozen polygon count exactly. It never substitutes an
  integration default or depends on an interactive control continuing to exist.
- The selected mode, topology, separation choice, source, output destination,
  and any optional instruction advertised by a product capability are frozen
  before a request enters the queue. An instruction control is absent when the
  runtime does not advertise that stable capability. Capability discovery is a
  strict, versioned, product-only manifest keyed by generation mode; a missing,
  malformed, or unknown capability fails closed and the host omits the
  instruction field. An advertised instruction is trimmed and limited to 1,000
  Unicode characters on both platforms.
- Every job has exactly one source image. Picking or dropping several images
  creates independent one-image sessions; references are never combined into a
  single 3D job. One picker/drop action accepts up to 100 images; each must be a
  PNG, JPEG, or WebP no larger than 25 MiB, 32,768 pixels on either axis, or
  64 million decoded pixels.
- The primary action submits only the selected session. Other sessions from the
  same import remain drafts; parallel work requires an explicit submission for
  each session.
- Every explicit submission creates a fresh dispatch identifier and therefore a
  fresh generation. Restart replay is recognized as the same request only when
  that dispatch identifier matches; source-content equality never makes two
  user submissions the same job.
- Each accepted explicit submission remains a distinct job. Start-response
  order cannot collapse requests or steal a newer user selection. The Windows
  primary action is hidden and disabled while its selected session is submitted
  or running; an unrelated draft can still be submitted while other work runs.
- A manually requested revision starts a fresh progress clock and ratio without
  inheriting its parent's execution identity or timing. The parent preview stays
  visible. Automatic separation continues the combined generation progress.
  Shared-settings captions name only the unsubmitted images actually affected;
  completed revisions never count as pending images.
- At most two jobs run simultaneously. 3D, SVG, and image jobs have independent
  concurrency limits.
- Runtime preparation is implementation-private. The public host observes only
  product job states, enforces the two-job limit, keeps capacity isolated by
  creation tool, and never exposes implementation identities, account state,
  or preparation mechanics through UI, diagnostics, fixtures, or IPC.
- Opening the surface paints the product UI before requesting readiness work.
  Idle surfaces do not poll jobs or history. Windows polls opaque readiness
  every two seconds while open and stops on page teardown; its host refreshes
  that status asynchronously so the header never blocks the UI. Android's
  matching readiness display port is outstanding. Job status and
  estimated-progress refreshes run only while accepted or recovered work is
  active; history refreshes on open, focus, history mutation, and terminal
  transitions.
- An accepted job has a two-hour whole-job watchdog. Silence and broken execution
  connections may fail earlier. Inbound elapsed and estimated-duration values
  are bounded to the same two-hour ceiling before storage or presentation.
- Progress preserves `queued`, `preparing`, `waiting_for_user`, `generating`,
  `segmenting`, `finalizing`, `done`, `failed`, and `cancelled`.
- When an accepted job needs a bounded user interaction, the runtime creates
  that interaction in a presentation-capable browser surface from the start,
  keeps the exact live surface hidden while passive completion remains
  possible, and reveals that same surface without transferring its URL,
  cookies, page state, or challenge state. The app shows only the product-level
  `waiting_for_user` state. Dismissing or timing out the interaction fails
  closed, cancellation closes it, and no consequential submission occurs
  before the interaction succeeds.
- The source preview, public stages, measured ETA, and final artifact are the
  generation visuals.
- Background preparation must never reveal a verification window or take
  focus. A deferred capacity verification is coalesced into one actionable
  request, presented as an `Action needed · 1` chip. Opening the chip explains
  the purpose using only creation-capacity language and offers Verify now or
  Later. Verify now explicitly starts a fresh verification; no expiring
  challenge is retained while the user defers. Repeated clicks share one
  operation. Closing the mini app cancels that operation. Status refreshes on
  open, focus, preparation completion, and while explicit verification runs.
  The badge does not promise additional credits or expose preparation details.
- Queue and history rows render a persisted, bounded project derivative when
  one is available and fall back to the shared image icon. They never decode
  an original source merely to paint a row. The selected source uses a bounded
  preview derivative. Original image bytes are never retained by the product
  surface. Session creation is immediate, generation remains available while
  the selected preview loads, and preview decoding never blocks the interaction
  thread. Project derivatives are at most 128 pixels on either axis and 12 KiB
  encoded; legacy rows hydrate them through a bounded two-lane cache.
- Android retains a persistable user-owned source handle when available and
  materializes full bytes only while the job is queued, running, or recoverable.
  History keeps a bounded presentation derivative, not a private full-size
  duplicate. Job copies are removed after the delivery/history transaction
  commits. If source access is later revoked, the result remains available and
  a new submission asks the user to select the source again; a thumbnail is
  never substituted as generation input.
- A successful result is a validated triangle GLB no larger than 100 MiB, with
  site-neutral naming. It preserves bounded skins and authored animation.
  Bounded animation clips support bone and
  node transforms and morph weights; playback starts paused and offers clip
  selection, play/pause, scrubbing, speed, and rest-pose reset. Geometry metadata
  is reported when known.
  Validation runs before publication and bounds the JSON, buffers, views,
  accessors, geometry, node graph, morph targets, materials, and both decoded
  image pixels and per-texture GPU work. The asset version is exactly 2.0 and
  JSON chunk padding is spaces only. Overlapping buffer views cannot multiply
  loader copies beyond the 100 MiB aggregate view budget. Accessors obey
  absolute component and vertex alignment, cover the complete loader allocation
  tail, and expose valid finite bounds. Renderer-consumed binary float data is
  finite and no greater than 10,000,000 in absolute value. Declared position
  bounds must contain the committed position values within a narrowly bounded
  numeric comparison tolerance: at most 1/32,768 world units or four binary32
  rounding units at the compared magnitude, whichever is greater. Values
  outside that tolerance remain invalid. Every `buffer.byteLength` is the exact
  logical committed byte count. An embedded data-URI buffer must decode to that
  exact length and use the resource type allowed by its field. GLB buffer 0 may
  add only the zero-filled 0–3 byte alignment suffix required to align its BIN
  chunk to four bytes. A BIN chunk exists exactly when it backs URI-less buffer
  0; orphan BIN chunks and later URI-less buffers are rejected. External
  resources are rejected.
- Every triangle primitive contains a whole number of triangles and every
  committed index is smaller than its position accessor. The selected scene
  exists, reaches geometry, and has globally unique roots; this prevents eager
  loaders from cloning the same subtree across scenes. Node depth is at most
  256, and node transforms and morph weights are finite and bounded.
  Indexed primitives that partition one shared position accessor charge that
  vertex store once; every index partition is still charged, and non-indexed
  vertex work is charged for every primitive and scene instance.
- Texture payloads must decode completely. Animated PNG and animated WebP are
  rejected. Material texture indices are validated, and every texture clone
  caused by a non-default UV channel or texture transform is charged to the
  same 32-million-pixel referenced-texture budget. Every numeric material value,
  including extension values, is a correctly typed JSON number with the
  required factor-vector arity, and is finite and bounded. Texture-transform
  values follow the same bound, and texture sampler filters and wrapping modes
  must be valid typed enum values.
- The product viewer does not expose authored cameras, so
  cameras, sparse accessors, cyclic or multi-parent node graphs,
  and non-triangle primitives are rejected rather than parsed as hidden work.
  Rest-pose skins are accepted only when the skin and joint totals are bounded,
  joint references are unique and in range, inverse-bind matrices are finite,
  every bound primitive has paired four-component joint and normalized weight
  accessors, every joint value is in range, and each vertex's weights sum to one
  within a narrow numeric tolerance. Skin scopes must own geometry and cannot
  be nested or ambiguous.
- GLB extensions fail closed. Only the explicitly versioned, non-amplifying
  material and embedded-WebP extension set in the shared fixture is accepted;
  declarations must be unique, required extensions must be a subset of used
  extensions, and every extension body must be a declared, allowlisted object.
  A future renderer upgrade cannot silently enable geometry compression,
  instancing, lights, or another unbudgeted feature.
- Presentation revalidates the committed bytes immediately before loading. The
  Windows custom-protocol response validates and serves one in-memory snapshot;
  Android loads only the revalidated app-owned result or bounded preview-cache
  materialization.
- Generation always validates, publishes, records, and displays its base model
  before optional separation. Quality drafts choose triangle or quad topology;
  legacy requests without a choice retain their earlier quad default. A request with separation produces the model as
  triangles because
  the separation operation does not accept quad input; topology is frozen with
  the request and is never silently changed after generation.
  When automatic separation is selected, it consumes that committed result's
  continuation in one child job; the base remains visible and saved while the
  child runs and remains usable if separation fails. Recovery starts that child
  at most once. An eligible quality result may expose a separate continuation
  for 24 hours when automatic separation was not requested.
- A quality quad result publishes its previewable GLB and quad-source FBX as one
  delivery transaction. Saved-result presentation names both files. Rename,
  delete, recovery, and ownership validation cover both artifacts together;
  publication fails rather than silently losing an expected companion.
  A separation-compatible triangle base has no FBX companion and its committed
  GLB remains visible while the separated child revision runs.
- Segmented output preserves existing meaningful part nodes. A single-mesh
  result may be expanded by disconnected components when that produces useful
  parts.
- Result topology statistics count shared vertex storage once. Distinct indexed
  primitives contribute their triangle faces once, while repeated presentation
  instances do not inflate either value.
- Durable history stores the frozen product settings and result metadata.
- Each imported creation is a project. Generation and every successful
  refinement create immutable revisions inside that project; a child revision
  names its parent and never replaces or mutates the parent artifact. A failed
  child keeps the parent selected, previewable, and downloadable.
- The current revision exposes only refinement actions proved available for its
  generation mode and artifact. Unsupported actions are absent rather than
  presented as disabled promises. A supported action may be disabled as a
  whole when its live, non-public allowance is temporarily unavailable; the
  panel then states that grayed actions need more creation capacity. The UI
  never exposes implementation brands, account balances, or credit counts.
  While a child revision is being created, the viewer keeps showing the parent
  artifact it was started from instead of returning to the empty placeholder.
- Only the current revision may consume a continuation. Segmented geometry is
  not a terminal workflow state: other advertised refinements may remain valid.
  The runtime owns action eligibility for the current revision; the host must
  not replace that contract with a generation-mode or separation-only gate.
  Actions still require a valid continuation and validated output.
  Refinement submission rechecks the current revision and its prerequisites.
  A recorded submission intent with an unknown outcome is never resubmitted;
  recovery follows the exact accepted operation, not another operation of the
  same type. Geometry-only quad revisions offer topology conversion first;
  material, separation, rig, and animation actions require compatible geometry.
  Existing materials and rigs must not be silently discarded by conversion.
  Interactive texture painting is outside this product. Animation success requires
  a resulting artifact with validated playable tracks; a successful remote job
  alone is insufficient.
- Recent sessions are presented newest-first on every surface, including newly
  imported, queued, processing, and saved-result rows. Presentation sorting
  never changes pending dispatch order.
- Deleting one saved result is immediate. Delete all requires confirmation,
  then removes every saved result for this mini app without affecting the
  other creation tools.
  Missing outputs are hidden; rename and delete operate on the real file.
- New results and revisions are committed to the app-managed project library,
  not automatically published to a public folder. The user explicitly exports
  only the selected revision to Downloads. Export validates the committed size
  and SHA-256 immediately before copying, includes a required companion using
  the same collision suffix, never overwrites an existing file, and verifies
  the published bytes. Exported copies are user-owned and never pruned by SGT.
  Legacy results already published to Downloads remain user-owned and keep
  their existing behavior.
- Output delivery is a durable transaction keyed by the dispatch identifier:
  validated result, prepared publication receipt, published location, history
  commit, then recovery-intent removal. Restart reconciliation resumes that
  transaction without repeating generation or publishing a second copy. A
  published file with uncertain ownership is retained for the user.
- History rename is also transactional. A failed index commit rolls the file
  move back, and restart reconciliation repairs an interrupted rename without
  orphaning the result.
- A terminal revision may be refined or its project may be used to start a new
  generation without mutating any previous revision.
- History uses the same user history limit as the eGUI/Android History feature:
  50 entries by default, configurable from 10 through 200 per creation tool,
  plus a shared 4 GiB managed-artifact budget. The newest result of each tool
  remains available; older entries are pruned by age until both limits hold.
  Pruning removes the
  corresponding app-owned artifact and derivatives exactly as explicit history
  deletion does. Cleanup is atomic, path-confined, retryable, and never removes
  a live, queued, accepted-recovery, user-exported, or externally modified
  artifact. Durable history stores the committed artifact size and digest so
  cleanup can prove it still owns the exact bytes before deletion.
- Managed admission also reserves at least 1 GiB of free volume space and the
  maximum bytes for the new result. Reclaimable data is pruned before dispatch;
  if live or recoverable data prevents that reserve, the request is rejected
  before generation starts.
- Legacy history rows without committed ownership proof remain user-owned.
  They are not hashed during startup and are never deleted automatically.
  Storage pruning runs before new import admission so reclaimable managed bytes
  cannot create a permanent full-storage dead end.

## Shared Visual System

- Windows and Android expose one provider-neutral generation flow. No
  implementation branding, hidden route selection, provider progress text, or
  provider error text appears.
- Android uses the shared native Material 3 Expressive creation shell and the
  same settings order, limits, stage mapping, preview math, and result metadata
  as Windows.
- A completed project focuses the selected revision, its preview and metadata,
  refinement controls, and one explicit Download action. Generation settings
  remain available for a new project but do not crowd the completed-project
  workspace.
- Windows and Android present validated results with the same versioned WebView
  viewer document. Each host exposes only the revalidated, app-owned GLB through
  an app-controlled local origin; the viewer cannot fetch external resources.
  Platform glue owns lifecycle, input routing, and safe-area sizing only. A
  platform-native renderer or silent fallback is not a supported deviation.
- Viewer controls cover orbit, zoom, pan, grid, wireframe, auto-rotate, toon,
  and outline. Wireframe and outline remain independent.
- Every preview shading mode renders both sides of validated triangles, so
  orbiting around open or inconsistently wound generated surfaces never makes
  them disappear.
- Filled Material Symbols Rounded come from the shared SGT icon catalog; the
  mini app does not carry a separate hand-authored icon set.
- An unchanged status refresh or thumbnail completion preserves existing interactive
  queue targets.
  Pointer hover, focus, and the first selection click cannot be invalidated by
  background reconciliation. Interactive orbit renders at full input cadence
  even when the history contains many items.

## Public Boundary

- Public source, fixtures, tests, diagnostics, history, and UI describe only
  product settings, jobs, stages, artifacts, and stable IPC state.
- Public requests carry product modes and advertised capability names only.
- Implementation details and raw errors remain outside this repository's
  contracts, diagnostics, history, and presentation.
- Every inbound event is normalized before storage, logging, or display.

## Failure And Recovery

- Submission and recovery are bounded and fail closed when acceptance cannot be
  proven.
- Runtime retries are bounded, cancellation-aware, and never duplicate a
  consequential submission whose outcome is uncertain. Exact retry,
  authentication, and capacity policies remain private to the runtime.
- An accepted job is never submitted twice during recovery.
- Restart recovery resumes the same dispatch without relying on source-content
  identity.
- If execution is lost after provider acceptance, the host retains the exact
  dispatch, request, and materialized input and requeues that same dispatch for
  recovery. It is not recorded as a terminal failure and no consequential
  submission is repeated.
- Cancellation wins over late success and a stale completion cannot affect a
  newer execution.
- A success event is emitted only after the output has been validated and
  committed.
- A prepared execution slot remains live until its assigned job finishes.
  Failed preparation retires that execution instance before the slot retries.
- A preparation instance whose rendering context stops responding is retired
  before retry. Recreated execution state must use platform services belonging
  to its own presentation surface so an app or system-runtime update cannot
  preserve a mismatched context indefinitely.
- A rendering operation that stops responding is bounded independently from the
  two-hour whole-job watchdog. The unresponsive execution instance is retired,
  and recovery never repeats a consequential action whose acceptance may have
  occurred.
- A recovery-reserved slot is checked against the exact request before
  assignment and cannot consume or fail an unrelated queued job.
- Every finished assignment retires its execution instance before that slot is
  prepared for another job.
- A recovery handoff publishes execution loss only after that retirement, so
  the same dispatch cannot race preparation against its previous worker state.
- A creation surface may contain multiple independent sessions. Closing it
  cancels its queued and running jobs, releases their owned execution resources,
  destroys the product surface, and prevents a late completion from publishing.

## Verification

- Shared fixture: `parity-fixtures/image-to-3d/state-contract.json`
- Windows routing tests verify flow limits, topology clamping, separation
  visibility, queue state, cancellation, result validation, and history.
- Android Full and Play tests read the same product fixture and verify session
  isolation, the preview contract, concurrency, and result behavior.
- Real UI acceptance submits the available generation flow on Windows, Android
  Full, and Android Play. It selects the source through the product surface,
  waits for a terminal state, and validates the committed GLB.
- A failed real UI case retains bounded terminal diagnostics, triggers private
  execution-readiness investigation and repair, then reruns in a fresh evidence
  directory. Acceptance is incomplete until the repaired build completes the
  real job and the committed GLB validates; repeated repair failures never
  cancel the required rerun.

## Platform Deviations

- The revised control groups and readiness presentation are Windows changes;
  Android's matching control presentation has not yet been ported or verified.

- The progress fixture's primary-action visibility, fresh manual-revision
  presentation, and clarified shared-settings caption are verified on Windows.
  Android presentation has not yet been ported or verified against this fixture.

- Both platforms keep new project revisions in their app-managed creation
  library. Explicit export uses the system Downloads directory on Windows and
  MediaStore Downloads on Android. Platform folder selection is not part of the
  3D project workflow.
- Windows presents a runtime-owned WebView2 interaction window for the bounded
  user-interaction state. Android currently preserves and fails closed on the
  same state contract but does not yet present the worker-owned WebView across
  its isolated process boundary; Android release acceptance must not treat a
  run that requires that interaction as passed.
- The deferred capacity-verification badge is currently Windows-only. Android
  does not advertise a verification action until its interactive host exists.
- Both platforms use the shared viewer document; only the WebView host and
  app-owned result-delivery adapter are platform-specific.
- Platform delivery differs, but product settings, progress, cancellation,
  artifact, continuation, and history behavior remains identical.
