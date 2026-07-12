---
name: manage-background-presets
description: Add, edit, remove, reorder, or change default Screen Recorder backgrounds while preserving preview/export parity.
allowed-tools: Bash, Read, Edit, Write, Glob, Grep
---

# Manage Recorder Backgrounds

## Source of Truth

`screen-record/src/config/shared-background-presets.json` owns built-in data, panel order, and default selection.

## Workflow

1. Confirm operation, preset ID, family, order/default effect, and visual constraints.
2. Search every consumer:

```powershell
rg -n 'shared-background-presets|backgroundPresets|BuiltInBackground|bg_style|bg_params|backgroundType' screen-record src\overlay\screen_record
```

3. Change shared data first. Existing families should remain data-only.
4. If a new family is required, update all three render boundaries from the same parameters:
   - preview/swatches: `screen-record/src/lib/renderer/builtInBackgrounds.ts`
   - native export mapping: `src/overlay/screen_record/native_export/background_presets.rs`
   - GPU export shader: `src/overlay/screen_record/gpu_export/shader.rs`
5. Keep types/default/reset paths aligned in `backgroundPresets.ts`, `App.tsx`, `BackgroundPanel.tsx`, and `videoController.ts`.
6. Generate swatches from preview math. Never hand-tune a separate thumbnail or export look.
7. Verify preview canvas, panel swatch, and exported frames at multiple aspect ratios.

From `screen-record/` run:

```powershell
npm run build
npm test
```

Then run the Rust validation required by `AGENTS.md` for changed export code. Report catalog, family, order/default, visual parity, and test results.
