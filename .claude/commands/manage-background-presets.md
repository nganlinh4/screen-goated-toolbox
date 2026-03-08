---
name: manage-background-presets
description: Add, edit, remove, or reorder built-in screen-record backgrounds while keeping preview, panel swatches, and export fully aligned
allowed-tools: Bash, Read, Edit, Write, Glob, Grep
---

# Manage Background Presets

Safely change built-in recorder backgrounds without breaking WYSIWYG.

## Inputs

Ask the user for any that were not already provided:

1. **Operation** — `add`, `edit`, `remove`, `reorder`, or `change-default`.
2. **Background ID** — e.g. `gradient8`.
3. **Family** — `linear`, `stacked-radial`, `diagonal-glow`, `edge-ribbons`, or a new family.
4. **Panel/default behavior** — whether this background should change `panelOrder` or `defaultId`.
5. **Visual intent** — key composition constraints such as edge-weighted vs center-weighted, quiet center, or video-safe areas.

## Steps

1. **Start from the shared catalog** in `screen-record/src/config/shared-background-presets.json`.
   - Treat this file as the single source of truth for built-in backgrounds.
   - Keep `defaultId`, `panelOrder`, and every built-in preset definition in this catalog.
   - Do not add new hardcoded preset colors or swatch styling elsewhere first.

2. **Search all consumers before editing**:
   ```
   rg -n "shared-background-presets|backgroundPresets|BuiltInBackground|bg_style|bg_params|backgroundType" screen-record src/overlay/screen_record .claude -S
   ```
   Confirm whether the change is data-only or requires a new background family implementation.

3. **Update the frontend catalog/types** in `screen-record/src/lib/backgroundPresets.ts`.
   - Keep the preset type definitions aligned with the shared JSON schema.
   - Keep `DEFAULT_BUILT_IN_BACKGROUND_ID` and `BUILT_IN_BACKGROUND_PANEL_ORDER` sourced from the shared catalog.

4. **Update preview rendering** in `screen-record/src/lib/renderer/builtInBackgrounds.ts`.
   - Existing families should stay driven by shared preset data only.
   - If adding a new family, implement its preview painter here.
   - Swatches must come from the same preview painter math. Do not hand-design a separate thumbnail look.

5. **Update export rendering only through the same parameter model**.
   - Load the shared catalog in `src/overlay/screen_record/native_export/background_presets.rs`.
   - Map the same preset fields into compositor uniforms there.
   - Mirror the same family math in `src/overlay/screen_record/gpu_export/shader.rs`.
   - If a family needs different math, change both sides from the same parameter names instead of tuning preview and export independently.

6. **Keep default and reset paths centralized**.
   Check these files when changing defaults or removing presets:
   - `screen-record/src/App.tsx`
   - `screen-record/src/components/sidepanel/BackgroundPanel.tsx`
   - `screen-record/src/lib/videoController.ts`

7. **Preserve WYSIWYG deliberately**.
   - Preview thumbnail, preview canvas, and export output must all represent the same composition.
   - Avoid center-only focal points unless the video-safe area was explicitly designed for them.
   - When a preset is intended to stay visible around a large video frame, bias interesting detail toward the edges.

8. **Verify**:
   ```
   cd screen-record && ./node_modules/.bin/tsc --noEmit
   cargo fmt
   ORT_SKIP_DOWNLOAD=1 cargo check --target x86_64-pc-windows-gnu
   ```
   Fix all warnings and errors before stopping.

9. **Report the result**.
   Summarize:
   - catalog changes
   - any new family implementation points
   - default/order changes
   - verification status

## Key Facts

- Built-in background data lives in exactly one catalog: `screen-record/src/config/shared-background-presets.json`.
- Panel order and default selection are behavior, not just presentation.
- Swatches should be generated from the preview renderer so square thumbnails still match landscape preview composition.
- Export reads the same shared data through `src/overlay/screen_record/native_export/background_presets.rs`.
- The GPU shader consumes family-level parameters through the existing `gradient_color*` and `bg_params*` uniform slots.
