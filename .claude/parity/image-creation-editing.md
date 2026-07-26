# Image Creation And Editing Parity

## Canonical Source

- Product contract:
  [state-contract.json](../../parity-fixtures/image-creation-editing/state-contract.json)
- Windows launcher and host: [image_creator](../../src/overlay/image_creator)
- Windows web surface: [image-creator-ui](../../image-creator-ui/src)
- Shared creation-app shell: [image-to-svg-ui](../../image-to-svg-ui/src)
- Shared result history: [generation_history.rs](../../src/overlay/generation_history.rs)
- Android native creation shell:
  [creation](../../mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/creation)

## Product Contract

- The app is named Create/edit image / 이미지 생성/편집 / Tạo/edit ảnh.
- Its stable tool identifier is `image`, its operation is
  `create_image_from_reference`, and its public job prefix is `image_`.
- The plus action creates one empty image session immediately. A session accepts
  a nonblank instruction with zero, one, or up to 20 ordered reference images.
- Picking or dropping several references adds them to the current configurable
  session. It does not create one job per reference.
- Submission freezes the session's ordered reference list, instruction, and
  output destination as exactly one queued job. Multiple sessions may be queued
  and run independently.
- Four isolated execution slots are maintained and at most two image jobs run
  simultaneously. Image work does not consume a 3D or SVG job slot.
- Public progress uses only `queued`, `preparing`, `uploading`, `generating`,
  `finalizing`, `done`, `failed`, and `cancelled`.
- Cancellation is monotonic. A late event cannot revive a cancelled job or
  publish an artifact for it.
- Recovery never duplicates an accepted request. A user retry creates a new job
  without mutating the prior result or history row.
- Success requires a nonempty PNG or WebP that decodes to positive dimensions.
  The artifact is committed atomically before success is emitted.
- Durable history stores the ordered reference list, instruction, result
  metadata, and completion time. Missing outputs are hidden; rename and delete
  operate on the published file.

## Shared Visual System

- Windows uses the same title bar, queue rail, stage, controls rail, status
  strip, dialogs, focus treatment, light/dark tokens, and reduced-motion rules
  as the Image to SVG and Image to 3D mini apps.
- Status polling preserves the instruction field's DOM identity, value,
  selection, focus, and active IME composition.
- Typography uses the shared locally served Google Sans Flex variable font with
  its rounded axis. No app-specific display face is introduced.
- Icons use filled Material Symbols Rounded through the established Windows and
  Android asset conventions.
- Android uses the existing native Material 3 Expressive creation components,
  shared typography, shared shapes, and adaptive result layout.
- The image stage has no white paper layer. Empty space remains transparent over
  the shared theme surface, and a single reference preserves its intrinsic
  aspect ratio instead of being forced into a fixed canvas ratio.
- Thumbnail and stage previews are bounded derivatives. Original image bytes
  are never retained by the WebView, preview hydration is nonblocking, and the
  session action remains available while previews load.

## Public Boundary

- Public source, fixtures, tests, diagnostics, history, and UI describe only
  product requests, product progress, artifacts, and stable IPC state.
- Implementation-specific mechanics and raw errors are excluded from this
  repository's contracts and presentation.
- Inbound runtime events are normalized to the public stages and feature-level
  copy before they are stored, logged, or shown.

## Failure And Recovery

- A malformed request, invalid supplied reference, invalid artifact, cancelled
  job, stale callback, or exhausted bounded recovery fails closed. An empty
  reference list is valid for image creation.
- Failures remain attached to the exact job that produced them.
- Closing the UI does not cancel or corrupt queued work.
- Output extraction may retry only for the already-created result; it does not
  repeat the user's creation request.

## Verification

- Shared fixture:
  `parity-fixtures/image-creation-editing/state-contract.json`
- Windows tests cover request routing, queue isolation, public event
  normalization, cancellation, artifact validation, and history.
- Android Full and Play tests read the same product fixture and verify four
  isolated image workers with a two-job concurrency ceiling.
- Frontend validation covers the shared font and icon system, public
  vocabulary, light/dark rendering, and focused Vietnamese IME input across
  polling intervals.

## Platform Deviations

- Windows writes to a selected filesystem folder. Android publishes through
  MediaStore or a persisted Storage Access Framework directory.
- A one-reference edit uses the before/after comparison. A text-only creation
  presents the result alone. A multi-reference edit presents the ordered
  reference set with the result rather than pretending that one reference is
  the sole before image.
- Platform delivery differs, but request, progress, cancellation, artifact, and
  history behavior remains identical.
