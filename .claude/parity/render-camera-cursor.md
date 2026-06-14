# Render Camera/Cursor Math Parity (Preview == Export / WYSIWYG)

This spec locks the camera/zoom/cursor render math so the TypeScript preview and
the Rust export stay bit-for-bit aligned (within 1e-6). Unlike the Windows<->mobile
parity specs, the two surfaces here are the **preview** (TypeScript, what the user
sees) and the **export** (Rust, what gets written to the file). Per the project
WYSIWYG contract, the preview is the single source of truth and the export must
reproduce it exactly.

## Canonical Source
- Canonical (preview) — camera/zoom: [screen-record/src/lib/renderer/cameraZoom.ts](../../screen-record/src/lib/renderer/cameraZoom.ts)
  - `calculateCurrentZoomStateInternal`, `zoomBlockEnvelope`, `easeCameraMove`,
    `toViewportCenter` / `fromViewportCenter`, `blendZoomStates`.
- Canonical (preview) — cursor primitives: [screen-record/src/lib/renderer/cursorDynamics.ts](../../screen-record/src/lib/renderer/cursorDynamics.ts)
  - `catmullRomInterpolate`, `normalizeAngleRad`, `lerpAngleRad`,
    `smoothDampScalar`, `springStepScalar`, `springStepAngle`, plus the
    `processCursorPositions` pipeline.
- Export (must match) — camera: [src/overlay/screen_record/native_export/camera_path.rs](../../src/overlay/screen_record/native_export/camera_path.rs)
- Export (must match) — cursor primitives: [src/overlay/screen_record/native_export/cursor_path/spring.rs](../../src/overlay/screen_record/native_export/cursor_path/spring.rs) and [.../cursor_path/processing.rs](../../src/overlay/screen_record/native_export/cursor_path/processing.rs)
- Export — Rust-only squish/click-fuse state machine: [src/overlay/screen_record/native_export/cursor_path/visibility.rs](../../src/overlay/screen_record/native_export/cursor_path/visibility.rs) + the simulation loop in [.../cursor_path/mod.rs](../../src/overlay/screen_record/native_export/cursor_path/mod.rs)

## Behavior Contract
- **Camera path** is a clean dual-language golden. For each fixture case the same
  `segment` + sample times produce the same `{zoom, posX, posY}` from both
  `calculateCurrentZoomStateInternal` (TS) and `calculate_zoom_state` (Rust) within
  1e-6. Cases cover: constant auto-zoom, moving auto path + influence ramp, a
  manual zoom block over the auto path (ease-in/hold/ease-out), a block -> auto-gap
  -> block sequence (the gap must revert to auto), the smootherStep ramp boundary
  with no auto path (blend against default), and a `followCursor` block.
  - The fixture also locks `zoomBlockEnvelope` directly at the ramp edges.
  - Identity contain-fit is assumed in the fixture (`crop: null`, canvas dims ==
    source dims), so the Rust anchor `posX/posY` equals the TS anchor output.
- **Cursor primitives** that have a clean TS twin are shared in the same golden:
  `catmullRom`, `normalizeAngle`, `lerpAngle`, `smoothDampScalar` (settle +
  overshoot-clamp), `springStepScalar` (under/critical/over damped), and
  `springStepAngle` (across the +-PI seam). Both languages step the same recurrence
  and must match each recorded step within 1e-6.
- **`smoothMousePositions` (the first pipeline stage) is now a shared
  golden.** The Rust blur was aligned to the canonical TS preview: a FIXED 3-pass
  UNIFORM box blur over the symmetric window `[max(0,i-half), min(n-1,i+half)]` with
  `half = floor(windowSize/2)` and `windowSize = smoothness*2+1` (each sample
  weighted 1, mean = running window sum / window length), plus the interpolated
  frame clamp `min(ceil(dur*fps), 60)`. The export previously used an
  exponential-weight Gaussian blur, `passes = ceil(windowSize/2)`, a `±windowSize`
  (≈2× too wide) half, and no frame clamp — diverging from the preview by 20-34px
  (worst at `smoothness=0`, where the preview is crisp/identity). The
  `smoothMousePositions` golden case (smoothness 0 / 5 / 10 over a representative
  track) locks both languages within 1e-6 and will catch any future regression.
  (The spring/wiggle/tilt stages of `processCursorPositions` are exercised through
  the `springStep*`/`smoothDamp*` primitive goldens above.)
- **The squish / click-fuse / release state machine is a Rust-only golden** (there
  is no single TS twin function; the equivalent preview logic lives inline in
  `drawFrame.ts`). It is covered by Rust `#[test]`s over `visibility.rs`, not by the
  cross-language fixture.

## Drift Fixed By This Spec
- `smoothDampScalar` zeroes the velocity on its overshoot-clamp branch (matching
  canonical Unity SmoothDamp). The Rust `smooth_damp_scalar` previously carried the
  un-recomputed velocity forward on that branch, so export diverged from preview on
  any frame where the heading smoother overshot its target. Rust was aligned to TS
  (the preview the user sees). The `smoothDampScalar.overshootClamp` golden case
  exercises this branch (a step with `value == target` and `velocity == 0`) and
  will catch any future regression in either language.

## Fixtures
- Shared cross-language golden: [parity-fixtures/render-camera-cursor/golden.json](../../parity-fixtures/render-camera-cursor/golden.json)
  - Generated from the canonical TS side by
    [screen-record/tests/unit/_generateRenderGolden.gen.ts](../../screen-record/tests/unit/_generateRenderGolden.gen.ts)
    (run with `npx vitest run --config vitest.gen.config.ts`). Never hand-edit the
    numbers; regenerate when the canonical TS math intentionally changes.
- TS assertions (canonical side reproduces the fixture):
  [screen-record/tests/unit/renderCameraCursorGolden.test.ts](../../screen-record/tests/unit/renderCameraCursorGolden.test.ts)
- TS pure-function unit tests:
  [screen-record/tests/unit/cursorDynamics.test.ts](../../screen-record/tests/unit/cursorDynamics.test.ts)
- Rust assertions (export must match the fixture within 1e-6):
  - camera: `tests` module in `camera_path.rs` (`camera_cases_match_golden`,
    `zoom_block_envelope_matches_golden`, plus pure-helper tests).
  - cursor primitives: `tests` modules in `cursor_path/spring.rs`
    (`*_matches_golden`, `smooth_damp_clamp_branch_zeros_velocity`, spring settling)
    and `cursor_path/processing.rs` (`catmull_rom_matches_golden`,
    `smooth_mouse_positions_matches_golden`).

## Deviations
- The cursor smoothing blur is now ALIGNED, not a deviation. `smoothMousePositions`
  (3-pass uniform box blur + `min(ceil(dur*fps), 60)` frame clamp) is reproduced
  bit-for-bit (within 1e-6) by the Rust export and golden-locked by the
  `smoothMousePositions` fixture case. The previously documented box-blur-vs-Gaussian
  /no-clamp divergence has been removed.
- The squish/click-fuse/release state machine has no single TS twin and is locked by
  Rust-only tests rather than the shared fixture.
