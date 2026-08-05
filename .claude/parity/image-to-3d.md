# Image to 3D Parity

## Canonical Source

- Windows launcher and host:
  [three_d_generator](../../src/overlay/three_d_generator)
- Windows creation UI: [3d-generator-ui](../../3d-generator-ui/src)
- Shared result history: [generation_history.rs](../../src/overlay/generation_history.rs)
- Shared fixture: [state-contract.json](../../parity-fixtures/image-to-3d/state-contract.json)
- Android creation UI:
  [creation](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation)

## Product Contract

- The app is named Image to 3D / 이미지를 3D로 / Ảnh sang 3D.
- Mode appears before topology:

  - Fast / 빠름 / Nhanh supports 100–15,000 polygons, returns a segmented
    result, and hides automatic separation.
  - Quality / 품질 / Tốt supports 500–20,000 polygons and offers optional
    automatic separation. Quality is the default.

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
- Rapid explicit presses on the selected session are captured as distinct jobs,
  including while an earlier submission is queued or running. Start-response
  order cannot collapse requests or steal a newer user selection.
- At most two jobs run simultaneously. 3D, SVG, and image jobs have independent
  concurrency limits.
- Readiness maintains an immediate reserve at least as large as the parallel-job
  limit. Accepted demand expands preparation proactively within a bounded
  capacity, and consuming prepared capacity starts background replenishment.
  Existing ready work never waits for replenishment, and demand in another
  creation tool cannot consume this tool's reserve.
- Readiness requires an actionable creation surface. A signed-in surface that
  is still blocked by first-use guidance is resolved before it can accept work.
- Each generation mode has an independently prepared execution lane. A lane
  that is slow or unavailable does not delay, revoke, or consume readiness for
  another mode, and independent lanes may prepare concurrently.
- Opening the surface paints the product UI before requesting readiness work.
  Idle surfaces do not poll jobs, history, or readiness. Job status and
  estimated-progress refreshes run only while accepted or recovered work is
  active; history refreshes on open, focus, history mutation, and terminal
  transitions.
- An accepted job has a two-hour whole-job watchdog. Silence and broken execution
  connections may fail earlier. Inbound elapsed and estimated-duration values
  are bounded to the same two-hour ceiling before storage or presentation.
- Progress preserves `queued`, `preparing`, `generating`, `segmenting`,
  `finalizing`, `done`, `failed`, and `cancelled`.
- The source preview, public stages, measured ETA, and final artifact are the
  generation visuals.
- Queue and history rows use the shared image icon and never decode artwork.
  The selected source uses a bounded preview derivative. Original image bytes
  are never retained by the product surface. Session creation is immediate,
  generation remains available while the selected preview loads, and preview
  decoding never blocks the interaction thread.
- Android retains a persistable user-owned source handle when available and
  materializes full bytes only while the job is queued, running, or recoverable.
  History keeps a bounded presentation derivative, not a private full-size
  duplicate. Job copies are removed after the delivery/history transaction
  commits. If source access is later revoked, the result remains available and
  a new submission asks the user to select the source again; a thumbnail is
  never substituted as generation input.
- A successful result is a validated, static triangle GLB no larger than
  100 MiB, with site-neutral naming. Geometry metadata is reported when known.
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
- Texture payloads must decode completely. Animated PNG and animated WebP are
  rejected. Material texture indices are validated, and every texture clone
  caused by a non-default UV channel or texture transform is charged to the
  same 32-million-pixel referenced-texture budget. Every numeric material value,
  including extension values, is a correctly typed JSON number with the
  required factor-vector arity, and is finite and bounded. Texture-transform
  values follow the same bound, and texture sampler filters and wrapping modes
  must be valid typed enum values.
- The product viewer does not play animation or expose authored cameras or
  skeletons, so animations, skins, cameras, sparse accessors, cyclic or
  multi-parent node graphs, and non-triangle primitives are rejected rather
  than parsed as hidden work.
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
- Fast results and quality results completed with automatic separation are
  already segmented. An eligible unsegmented quality result may expose a
  separate continuation for 24 hours.
- Segmented output preserves existing meaningful part nodes. A single-mesh
  result may be expanded by disconnected components when that produces useful
  parts.
- Result topology statistics count shared vertex storage once. Distinct indexed
  primitives contribute their triangle faces once, while repeated presentation
  instances do not inflate either value.
- Durable history stores the frozen product settings and result metadata.
- Recent sessions are presented newest-first on every surface, including newly
  imported, queued, processing, and saved-result rows. Presentation sorting
  never changes pending dispatch order.
- Deleting one saved result is immediate. Delete all requires confirmation,
  then removes every saved result for this mini app without affecting the
  other creation tools.
  Missing outputs are hidden; rename and delete operate on the real file.
- The default destination is the app-managed creation library, so its exact
  unchanged results are reclaimed with history retention just like SGT History
  media and Recorder projects. Choosing an external folder is an explicit
  export; that copy is user-owned and is never removed automatically.
- Output delivery is a durable transaction keyed by the dispatch identifier:
  validated result, prepared publication receipt, published location, history
  commit, then recovery-intent removal. Restart reconciliation resumes that
  transaction without repeating generation or publishing a second copy. A
  published file with uncertain ownership is retained for the user.
- History rename is also transactional. A failed index commit rolls the file
  move back, and restart reconciliation repairs an interrupted rename without
  orphaning the result.
- A terminal item may be reconfigured and submitted as a new job without
  mutating its previous result.
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

- Windows and Android expose only Fast and Quality. No implementation branding,
  selection, progress text, or error text appears.
- Android uses the shared native Material 3 Expressive creation shell and the
  same settings order, limits, stage mapping, preview math, and result metadata
  as Windows.
- Viewer controls cover orbit, zoom, pan, grid, wireframe, auto-rotate, toon,
  and outline. Wireframe and outline remain independent.
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
- An accepted job is never submitted twice during recovery.
- Restart recovery resumes the same dispatch without relying on source-content
  identity.
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
- A recovery-reserved slot is checked against the exact request before
  assignment and cannot consume or fail an unrelated queued job.
- Every finished assignment retires its execution instance before that slot is
  prepared for another job.
- A creation surface may contain multiple independent sessions. Closing it
  cancels its queued and running jobs, releases their owned execution resources,
  destroys the product surface, and prevents a late completion from publishing.

## Verification

- Shared fixture: `parity-fixtures/image-to-3d/state-contract.json`
- Windows routing tests verify mode limits, topology clamping, separation
  visibility, queue state, cancellation, result validation, and history.
- Android Full and Play tests read the same product fixture and verify session
  isolation, the preview contract, concurrency, and result behavior.
- Real UI acceptance submits one Fast and one Quality job on Windows, Android
  Full, and Android Play. It selects the source and mode through the product
  surface, waits for a terminal state, and validates the committed GLB.
- A failed real UI case retains bounded terminal diagnostics, triggers private
  execution-readiness investigation and repair, then reruns in a fresh evidence
  directory. Acceptance is incomplete until the repaired build completes the
  real job and the committed GLB validates; repeated repair failures never
  cancel the required rerun.

## Platform Deviations

- Both platforms default to the bounded app-managed creation library. Windows
  may export to a selected filesystem folder; Android may export through
  MediaStore or a persisted Storage Access Framework directory.
- Each platform may use its native rendering primitive while preserving the
  same adaptive result-surface behavior.
- Platform delivery differs, but product settings, progress, cancellation,
  artifact, continuation, and history behavior remains identical.
