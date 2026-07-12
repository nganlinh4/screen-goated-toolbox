# Preview/Export Render Parity

The TypeScript preview is canonical. Rust export must reproduce camera, zoom, and cursor math within `1e-6` for shared golden cases.

## Owners

- Preview camera: [cameraZoom.ts](../src/lib/renderer/cameraZoom.ts)
- Preview cursor: [cursorDynamics.ts](../src/lib/renderer/cursorDynamics.ts)
- Export camera: [camera_path.rs](../../src/overlay/screen_record/native_export/camera_path.rs)
- Export cursor: [cursor_path](../../src/overlay/screen_record/native_export/cursor_path)
- Shared golden: [golden.json](../../parity-fixtures/render-camera-cursor/golden.json)

## Contract

- Identical segment/sample input yields identical `{zoom, posX, posY}` across preview and export.
- Golden cases cover auto zoom, moving paths, manual blocks and ramps, auto gaps, default blending, and follow-cursor behavior.
- Shared cursor primitives cover Catmull-Rom interpolation, angle normalization/interpolation, damped scalar motion, and scalar/angle springs.
- Cursor smoothing is three passes of a uniform box blur over the same symmetric window, with the same interpolation-frame clamp and `<2px` idle-skip guard.
- Squish/click-fuse/release has no standalone TypeScript twin; Rust tests lock that export state machine while preview behavior remains visually checked.

## Change Procedure

1. Change preview math first.
2. Regenerate the golden; never hand-edit its numbers:

```powershell
Push-Location screen-record
npx vitest run --config vitest.gen.config.ts
Pop-Location
```

3. Port the same formula and parameters to Rust export.
4. Run the TypeScript golden/unit tests and Rust tests named beside the export implementations.
5. Compare real preview and exported frames at multiple aspect ratios before accepting the change.

Generator: [\_generateRenderGolden.gen.ts](../tests/unit/_generateRenderGolden.gen.ts). TypeScript assertion: [renderCameraCursorGolden.test.ts](../tests/unit/renderCameraCursorGolden.test.ts).
