# Image to SVG Parity

## Canonical Source

- Windows launcher and host: [image_to_svg](../../src/overlay/image_to_svg)
- Windows web surface: [image-to-svg-ui](../../image-to-svg-ui/src)
- Shared result history: [generation_history.rs](../../src/overlay/generation_history.rs)
- Runtime process contract: [creation_runtime.rs](../../src/overlay/creation_runtime.rs)
- Shared fixture: [state-contract.json](../../parity-fixtures/image-to-svg/state-contract.json)

## Behavior Contract

- One picker or drop may add multiple images as one batch. A batch shares its pre-submit model;
  a later batch can use a different model.
- Simple maps to the Classic model and costs two credits. Detail maps to Ultra and costs four.
- At most two jobs run concurrently. Two independently authenticated accounts stay prepared, and
  an account is reusable while at least four credits remain.
- Progress preserves draft, queued, preparing, generating, finalizing, done, failed, and cancelled
  states and uses measured timing estimates when available.
- Completion renders the real SVG at its intrinsic ratio and animates every path with adaptive,
  overlapping timing rather than rasterizing or truncating the path set.
- Viewer controls include fit, zoom, pan, background switching, path selection, fill/stroke edits,
  undo, redo, shape deletion, and saving edits back to the real SVG.
- Result history persists across sessions, lists only results whose output still exists, and can
  rename or delete the real output file.
- Android stages the canonical Windows web surface and changes only window, picker, storage, and
  runtime bridge behavior. Full and Play expose the same app and state machine; no executable code
  is downloaded by either flavor.

## Failure And Recovery

- A job never reports success before a newly generated SVG has been distinguished from prior
  dashboard results and written successfully.
- Authentication, credit, timeout, worker, and generation failures remain failed states and do not
  substitute a previous image's result.
- Cancellation targets one job. Closing the UI does not corrupt active jobs or persisted history.
- Preparation reuses eligible accounts and is staggered to avoid unnecessary mailbox churn.
- Fresh-account preparation is serialized across creation tools, and a mailbox rate limit pauses
  all new preparation attempts for five minutes without blocking already-ready workspaces. Remote
  preparation starts are always at least one minute apart, including after fast failures.

## Fixtures

- `parity-fixtures/image-to-svg/state-contract.json`
- Android JVM parity tests read the same fixture.

## Deviations

- Windows writes directly to a filesystem folder. Android publishes output through MediaStore or
  a persisted Storage Access Framework directory and represents it by a content URI.
- Android uses isolated WebView worker processes in place of the Windows native browser sidecar.
- Android currently uses the canonical in-app progress scene without the optional Depth Anything 3
  depth-bin preview pass; generated SVG rendering and editing are unchanged.
