# Image to SVG Parity

## Canonical Source

- Windows launcher and host: [image_to_svg](../../src/overlay/image_to_svg)
- Windows web surface: [image-to-svg-ui](../../image-to-svg-ui/src)
- Shared result history: [generation_history.rs](../../src/overlay/generation_history.rs)
- Shared fixture: [state-contract.json](../../parity-fixtures/image-to-svg/state-contract.json)
- Android native creation shell:
  [creation](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation)

## Product Contract

- One picker or drop may add multiple images as a batch.
- Every SVG job has exactly one source image. A multi-image batch creates
  independent one-image sessions; references are never combined into one job.
- A batch freezes Simple or Detail before entering the queue. A later batch may
  use a different choice.
- Simple and Detail are stable product choices. Their implementation mapping
  and capacity policy remain outside the public contract.
- At most two jobs run concurrently. Two independent execution slots stay
  prepared without consuming 3D or image-editing job slots.
- Progress preserves `draft`, `queued`, `preparing`, `generating`,
  `finalizing`, `done`, `failed`, and `cancelled`.
- While a job runs, the source may separate into six animated depth bins.
  Preview setup is silent, nonblocking, and cannot fail the creation job.
- Queue thumbnails and the selected source use bounded preview derivatives.
  Original image bytes are never retained by the WebView, session creation is
  immediate, and generation remains available while previews load.
- Completion renders the real SVG at its intrinsic ratio and animates the full
  path set with adaptive overlapping timing.
- Viewer controls include fit, zoom, pan, background switching, path selection,
  fill/stroke editing, undo, redo, shape deletion, and saving edits to the real
  SVG.
- History lists only existing outputs. Rename and delete operate on the
  published file.

## Shared Visual System

- Windows uses the shared creation-app title bar, queue rail, stage, controls,
  dialogs, focus treatment, typography, icon system, light/dark tokens, and
  reduced-motion behavior.
- Android uses the shared native Material 3 Expressive creation shell and
  preserves the same settings order, progress states, preview math, result
  controls, and history behavior.
- Polling never replaces a focused input or active IME composition.

## Public Boundary

- Public source, fixtures, tests, diagnostics, history, and UI describe only
  product settings, jobs, stages, artifacts, and stable IPC state.
- Implementation-specific mechanics and raw errors remain outside this
  repository's public contracts and presentation.
- Every inbound event is normalized before storage, logging, or display.

## Failure And Recovery

- A failed job remains failed and cannot publish a stale or previous result.
- Submission and recovery are bounded; an accepted request is never submitted
  twice.
- Cancellation wins over late success and stale callbacks cannot affect newer
  worker assignments.
- Closing the UI does not cancel or corrupt queued work.

## Verification

- Shared fixture: `parity-fixtures/image-to-svg/state-contract.json`
- Windows tests verify queue behavior, stage mapping, SVG validation, editor
  operations, cancellation, and history.
- Android Full and Play tests read the same product fixture and verify the
  native shell, worker isolation, preview contract, SVG result surface, and
  history behavior.

## Platform Deviations

- Windows writes to a selected filesystem folder. Android publishes through
  MediaStore or a persisted Storage Access Framework directory.
- Android renders SVG output in a sandboxed native-owned document surface;
  Windows uses the desktop web surface.
- Platform delivery differs, but product settings, progress, cancellation,
  artifact, editing, and history behavior remains identical.
