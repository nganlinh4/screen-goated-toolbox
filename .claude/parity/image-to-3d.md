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
- Successful output is an ordinary GLB without `EXT_meshopt_compression` or
  `KHR_mesh_quantization`, has a site-neutral filename, and reports face/vertex counts when known.
- The viewer supports orbit, zoom, pan, grid, wireframe, auto-rotate, toon shading, and outline.
- Result history persists across sessions, lists only results whose output still exists, and can
  rename or delete the real output file.
- Android stages the canonical Windows web surface and changes only window, picker, storage, and
  runtime bridge behavior. Full and Play expose the same app and state machine; no executable code
  is downloaded by either flavor.

## Failure And Recovery

- A job never reports success before its output exists and passes the requested segmentation and
  GLB-normalization checks.
- Worker loss, timeout, rejected authentication, exhausted credits, or missing segmentation is a
  failed state with a retryable user-facing error; another healthy worker may accept later work.
- Cancellation targets one job. Closing the UI does not corrupt active jobs or persisted history.
- Preparation is bounded and staggered; it does not repeatedly replace a valid mailbox or account.
- Fresh-account preparation is serialized across creation tools, and a mailbox rate limit pauses
  all new preparation attempts for five minutes without blocking already-ready workspaces. Remote
  preparation starts are always at least one minute apart, including after fast failures.

## Fixtures

- `parity-fixtures/image-to-3d/state-contract.json`
- Android JVM parity tests read the same fixture.

## Deviations

- Windows writes directly to a filesystem folder. Android publishes output through MediaStore or
  a persisted Storage Access Framework directory and represents it by a content URI.
- Android uses isolated WebView worker processes in place of the Windows native browser sidecar.
- Android currently uses the canonical in-app progress scene without the optional Depth Anything 3
  preview pass; the generated GLB and its real 3D viewer are unchanged.
