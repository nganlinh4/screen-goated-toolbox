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
- The primary action submits only the selected session. Other sessions from the
  same import remain drafts; parallel work requires an explicit submission for
  each session.
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
  immediate, generation remains available while previews load, and preview
  decoding never blocks the WebView message thread. Only the selected or
  near-visible queue entries hydrate; off-screen history waits until it
  approaches the viewport, and hydration yields while the canvas is active.
- Completion renders every path in the real SVG at its intrinsic ratio. The
  entrance effect animates at most 120 evenly sampled paths with adaptive
  overlapping timing; all remaining paths are visible immediately so a large
  document cannot monopolize the UI thread.
- Viewer controls include fit, zoom, pan, background switching, path selection,
  fill/stroke editing, undo, redo, shape deletion, and saving edits to the real
  SVG.
- History lists only existing outputs. Rename and delete operate on the
  published file.

## Shared Visual System

- Windows uses the shared creation-app title bar, queue rail, stage, controls,
  dialogs, focus treatment, typography, icon system, light/dark tokens, and
  reduced-motion behavior.
- Filled Material Symbols Rounded come from the shared SGT icon catalog; the
  mini app does not carry a separate hand-authored icon set.
- Android uses the shared native Material 3 Expressive creation shell and
  preserves the same settings order, progress states, preview math, result
  controls, and history behavior.
- An unchanged poll or thumbnail completion preserves existing queue DOM.
  Polling never replaces a hovered selection target, focused input, or active
  IME composition. The queue owns its overflow, and the primary action remains
  reachable regardless of queue length.

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
- Closing the mini app cancels all of its queued and running jobs, terminates
  their tracked process trees, destroys the WebView, and prevents a late
  completion from publishing. Shared proactive preparation remains app-owned
  and is not mistaken for a mini-app job.

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
