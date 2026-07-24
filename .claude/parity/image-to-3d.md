# Image to 3D Parity

## Canonical Source

- Windows launcher and host: [three_d_generator](../../src/overlay/three_d_generator)
- Windows web surface: [3d-generator-ui](../../3d-generator-ui/src)
- Shared result history: [generation_history.rs](../../src/overlay/generation_history.rs)
- Runtime process contract: [creation_runtime.rs](../../src/overlay/creation_runtime.rs)
- Shared fixture: [state-contract.json](../../parity-fixtures/image-to-3d/state-contract.json)

## Behavior Contract

- The app is named Image to 3D / 이미지를 3D로 / Ảnh sang 3D.
- One picker or drop may add multiple images as one batch. A batch shares its pre-submit
  polycount and automatic-separation settings; adding another batch keeps independent settings.
- Polycount is clamped to `100..20000`, defaults to `5000`, and uses topology mesh generation.
- A generic mode selector appears immediately before topology:
  - `fast` / Fast / 빠름 / Nhanh selects the fast provider, supports `100..15000` polygons,
    always returns native parts, and hides automatic separation.
  - `quality` / Quality / 품질 / Tốt selects the quality provider, supports `500..20000`
    polygons, and keeps automatic separation as an independent opt-in.
- Quality is the default. Changing topology or automatic separation never changes mode. Changing
  mode clamps topology only to the selected mode's supported range.
- The explicit mode and its internal provider are frozen in request, progress, result, history,
  diagnostics, and paid recovery. The host and private runtime validate the chosen mode's limits
  without smart switching or replacing it.
- Provider identity remains an internal routing detail. Windows and Android render only the
  generic Fast/Quality choices, never provider names or provider-branded progress and error text.
- At most two jobs run concurrently. Four independently authenticated workspaces are prepared
  and retained without replacing still-eligible accounts.
- Quality-mode automatic part separation is submitted to its selected provider. A completed
  unsegmented quality result also exposes a separate-parts continuation; detailed `15+ parts` is
  the canonical continuation mode.
- A Tripo workspace that used part separation is retired. An unsegmented Tripo workspace retains
  its owning result's separation continuation for 24 hours even when it lacks the credits or
  readiness required for a new generation. If it is still generation-eligible, a new generation
  may reuse it and atomically invalidate that workspace's older continuation.
- Meshy generation costs five credits. Two independently reusable Meshy workspaces are created
  lazily (enough for the two-job concurrency ceiling), retained while authenticated and holding
  at least five credits, and replaced only when invalid or exhausted.
- Progress preserves preparing, generating, segmenting, finalizing, done, failed, and cancelled
  states. Estimates carry their measured sample count and use bounded persisted measurements when
  available; hard-coded provider durations are only the zero-sample fallback.
- While a job runs, the source becomes a Depth Anything 3 relief preview when the shared,
  on-demand model and ONNX Runtime are ready. First-use setup and inference run independently of
  remote generation, are serialized across creation jobs, stay visually silent until a preview is
  ready, and never turn preview failure into job failure.
- Successful output is an ordinary GLB without `EXT_meshopt_compression` or
  `KHR_mesh_quantization`, has a site-neutral filename, and reports face/vertex counts when known.
- A natively segmented GLB is render-ready before commit: triangle primitives have normals, an
  artifact with one mesh-bearing node expands its disconnected indexed components into explicit
  `part_###` nodes, and an artifact that already has multiple part nodes preserves those provider
  boundaries. Windows and Android apply the same rule.
- The Windows viewer defensively applies the same normal/part repair to legacy history files that
  predate runtime-side preparation. A completed in-session item remains selected while its durable
  history row catches up, and `stage=done` transitions directly from the source preview to the model.
- Meshy's encrypted `MESHY.AI` artifact is decrypted only inside the authenticated Meshy page
  origin. The runtime validates the decrypted GLB before normalization and never treats the
  encrypted container as a GLB.
- The viewer supports orbit, zoom, pan, grid, wireframe, auto-rotate, toon shading, and outline.
  Wireframe and outline are independent functional render controls on both platforms.
- Result history persists across sessions, lists only results whose output still exists, and can
  rename or delete the real output file. Its frozen metadata includes generation mode and provider.
- A done, failed, or cancelled item may be selected, reconfigured, and submitted again without
  mutating its previous durable output or history row.
- Android presents the canonical job, settings, viewer, history, and continuation states through
  an adaptive native Kotlin Compose Material 3 Expressive surface. The public app owns only the
  frontend, IPC, storage, and delivery host. The separately built creation runtime owns browser
  automation, account state, GLB conversion, and depth inference.

## Failure And Recovery

- A job never reports success before its output exists and passes the requested segmentation and
  GLB-normalization checks, including renderable normals and explicit part nodes for segmented
  output.
- A conflicting or stale client request cannot override mode: the host and runtime independently
  validate the explicit mode/provider pair, clamp polygon count to that mode's limits, force
  automatic separation off only for fast mode, and never choose a provider from polygon count.
- Paid-job identity includes the frozen provider and mode, source content rather than its path or
  timestamps, prompt, polycount, and model version. Matching jobs are serialized across workers
  and processes.
- Before a five-credit Meshy submission, the runtime durably records the generation intent and
  credit reservation. An ambiguous submission response fails closed; an accepted task is resumed
  on its owning account without resubmission, credit re-debit, or fallback to another account.
- Before a quality-provider submission, the generation control must be enabled, accept pointer
  events, and be the topmost element at its click point. A click attempt and a confirmed generation
  start are distinct durable states. Confirmation requires a task URL, an active generating state,
  or an observed credit debit. A native click may fall back once to a DOM click only while the same
  control remains ready; an unconfirmed outcome then fails closed without changing accounts or
  resubmitting. Recovery resumes only on the owning account, and failures before any click alone
  may use another prepared workspace.
- A validated GLB is committed atomically and a durable completion receipt is written before the
  host reports success. On Android the receipt points to a private-runtime-owned recovery artifact,
  never the public host's staging path. Pending, unknown, and completed recovery records remain
  replayable for seven days; Android redirects recovery to the worker that owns the recorded
  account slot.
- A validated quality-provider artifact may complete an unsegmented result while its generation
  operator is still finalizing. Automatic separation and a later separation continuation use a
  stronger gate: the same task must expose a validated artifact and terminal-success generation
  status before the runtime opens or submits separation. Queued, running, and unknown status remain
  generating, terminal failure fails the job, and page text alone cannot bypass this gate.
- Worker loss, timeout, rejected authentication, exhausted credits, or missing segmentation is a
  failed state with a retryable user-facing error; another healthy worker may accept later work.
  Binder death and service disconnection fail the worker's active job exactly once. A callback from
  an old assignment cannot release, fail, or complete a worker's newer assignment.
- Cancellation targets one job. Once cancellation wins the terminal transition, a late success
  cannot publish output, delete an earlier result, or replace the cancelled state. Closing the UI
  does not corrupt active jobs or persisted history.
- Preparation is bounded and staggered; it does not repeatedly replace a valid mailbox or account.
- Preparation progress remains below generation progress and failed preparation is captured in a
  bounded, privacy-safe local diagnostic journal.
- Fresh-account preparation is serialized across creation tools, and a mailbox rate limit pauses
  all new preparation attempts for five minutes without blocking already-ready workspaces. Remote
  preparation starts are always at least one minute apart, including after fast failures.

## Fixtures

- `parity-fixtures/image-to-3d/state-contract.json`
- Android JVM parity tests read the same fixture.

## Deviations

- Windows writes directly to a filesystem folder. Android publishes output through MediaStore or
  a persisted Storage Access Framework directory and represents it by a content URI.
- Android runs the separately delivered runtime behind the same isolated worker-process IPC. Full
  downloads a checksum-pinned DEX/native bundle from the runtime-bundles release; Play packages
  the same private runtime build in an on-demand dynamic feature.
- Full and Play use the same explicit-mode routing, Meshy-decryption, account-reuse, and result
  contracts. Only delivery differs: Full may install the checksum-pinned runtime bundle, while
  Play must package the runtime AAR in its on-demand feature and may not download executable code.
  Their pinned runtime manifest, version, feature contract, and payload content come from the same
  verified private-runtime build.
- Android renders completed GLB files natively with SceneView/Filament instead of Three.js.
- Android's native M3E presentation intentionally differs from the Windows desktop layout while
  preserving the same fixture-backed behavior contract.
- Android downloads the checksum-verified Depth Anything 3 Small model as removable data and uses
  the shared flavor-specific ONNX Runtime delivery. Inference remains inside the private creation
  runtime. It keeps the 518-pixel map in app cache rather than expanding it to source resolution.
