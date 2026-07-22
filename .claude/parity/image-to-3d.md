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
- Polycount is clamped to `500..20000`, defaults to `5000`, and uses topology mesh generation.
- At most two jobs run concurrently. Four independently authenticated workspaces are prepared
  and retained without replacing still-eligible accounts.
- Automatic part separation is opt-in. A completed unsegmented job exposes a separate-parts
  continuation; detailed `15+ parts` is the canonical separation mode.
- A workspace that used part separation is retired. An unsegmented workspace remains reusable
  while its credits and 24-hour generation window permit another run.
- Progress preserves preparing, generating, segmenting, finalizing, done, failed, and cancelled
  states and uses measured timing estimates when available.
- While a job runs, the source becomes a Depth Anything 3 relief preview when the shared,
  on-demand model and ONNX Runtime are ready. First-use setup and inference run independently of
  remote generation, are serialized across creation jobs, stay visually silent until a preview is
  ready, and never turn preview failure into job failure.
- Successful output is an ordinary GLB without `EXT_meshopt_compression` or
  `KHR_mesh_quantization`, has a site-neutral filename, and reports face/vertex counts when known.
- The viewer supports orbit, zoom, pan, grid, wireframe, auto-rotate, toon shading, and outline.
- Result history persists across sessions, lists only results whose output still exists, and can
  rename or delete the real output file.
- Android presents the canonical job, settings, viewer, history, and continuation states through
  an adaptive native Kotlin Compose Material 3 Expressive surface. The public app owns only the
  frontend, IPC, storage, and delivery host. The separately built creation runtime owns browser
  automation, account state, GLB conversion, and depth inference.

## Failure And Recovery

- A job never reports success before its output exists and passes the requested segmentation and
  GLB-normalization checks.
- Worker loss, timeout, rejected authentication, exhausted credits, or missing segmentation is a
  failed state with a retryable user-facing error; another healthy worker may accept later work.
- Cancellation targets one job. Closing the UI does not corrupt active jobs or persisted history.
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
- Android renders completed GLB files natively with SceneView/Filament instead of Three.js.
- Android's native M3E presentation intentionally differs from the Windows desktop layout while
  preserving the same fixture-backed behavior contract.
- Android downloads the checksum-verified Depth Anything 3 Small model as removable data and uses
  the shared flavor-specific ONNX Runtime delivery. Inference remains inside the private creation
  runtime. It keeps the 518-pixel map in app cache rather than expanding it to source resolution.
