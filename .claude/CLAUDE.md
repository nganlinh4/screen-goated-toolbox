# Screen Goated Toolbox

Windows AI productivity automation tool built with Rust.

## Project Context
- **Type**: Native Windows desktop application
- **GUI**: egui/eframe with glow renderer
- **Audio**: WASAPI, cpal, symphonia for multi-format playback
- **GPU**: wgpu for rendering, DirectML for ML inference
- **AI**: Parakeet for speech recognition, multiple AI provider integrations

## Build Commands
```bash
cargo build --release     # Production build
cargo run                 # Debug run
cargo clippy             # Lint check
cargo fmt                # Format code
cargo test               # Run tests
```

## Key Dependencies
- `egui` / `eframe`: Immediate mode GUI
- `parakeet-rs`: Local speech recognition with DirectML
- `ort`: ONNX Runtime for ML inference
- `windows-capture`: Screen capture API
- `wry`: WebView for markdown rendering
- `tray-icon`: System tray integration

## Code Patterns
- Use `anyhow::Result` for error handling
- Windows API via `windows` crate (0.62)
- Async audio processing with `parking_lot` mutexes
- Node graph workflows via `egui-snarl`

## File Size Limits
- **Maximum 600 lines per file** - if a file approaches this limit, split it into a module directory
- When splitting: `foo.rs` → `foo/mod.rs` + `foo/submodule.rs`
- Keep public API in `mod.rs`, move implementation details to submodules
- Prefer logical splits (e.g., `paint.rs`, `messages.rs`, `window.rs`) over arbitrary line-based splits
- Each submodule should have a clear, single responsibility

## Testing
- Always run `cargo clippy --all-targets` before commits
- Test on Windows 10/11 for compatibility
- For WSL validation, use `ORT_SKIP_DOWNLOAD=1 cargo check --target x86_64-pc-windows-gnu` (the `ort-sys` crate does not provide downloadable binaries for `x86_64-pc-windows-gnu`).

## Claude Code Rules
- **Never run `cargo build --release`** - the user will build manually when ready
- Use `cargo check` or `cargo clippy` for verification instead
- **Always fix all warnings** - code must compile with zero warnings
- **Never use `#[allow(dead_code)]`** - remove unused code instead of suppressing warnings
- Keep this `CLAUDE.md` updated whenever stable workflow/process knowledge changes.
- Do not add volatile details that are likely to change often; update when needed, not routinely.

## Model Catalog Workflow
- **Adding/editing/removing a model** — use `/manage-model-catalog` (see `.claude/commands/manage-model-catalog.md`)

## Frontend (screen-record) Rules
- **Always add descriptive class names** to JSX elements for DevTools debugging (e.g., `className="zoom-track ..."`, `className="text-segment ..."`)
- Class names should be semantic, kebab-case, and describe the element's purpose
- This applies to all components — tracks, handles, labels, overlays, buttons, etc.
- **Preview = Export (WYSIWYG)**: The frontend preview must always be streamlined and baked/calculated for the backend export. What the user sees in preview must be completely identical to the exported result. Minimize work/changes in the export/render backend when adding new features or changing things in the frontend — keep the preview as the single source of truth.

### Background WYSIWYG Contract
- For any new background preset/effect, do not maintain separate "look tuning" paths for preview and export.
- Implement one shared parameter model (colors, stops, glow centers/radii/intensity, vignette) and consume that model in both:
  - `screen-record/src/lib/videoRenderer.ts`
  - `src/overlay/screen_record/gpu_export.rs`
- If exact canvas primitives cannot match shader behavior, preview must render a cached per-pixel texture using the same math/parameters as export.
- For each new/changed background, verify with side-by-side screenshot: one preview frame and one exported frame at same timestamp.

### Cursor Collection Onboarding Checklist (Critical)
- New cursor collections must be generated as **single-cursor SVG files per type** (`cursor-*.svg`), never as full spritesheet content inside each file.
- Keep cursor file format consistent with existing stable packs:
  - final canvas `44x43`
  - explicit clipping to final canvas (to prevent overflow/stacking in export renderers)
  - one cursor glyph visible per file only
- If a cursor file embeds source art with a large inner `viewBox` (e.g. 308x288), apply per-cursor position offsets in **final canvas pixel space** (outer 44x43 placement, e.g. nested `<svg x/y>`), not by translating inner source coordinates.
- Cursor Lab offset values are measured in the lab preview canvas (`86x86`), not in final SVG canvas units (`44x43`).  
  When baking Lab offsets into SVG `x/y`, convert first:  
  `svgOffsetX = labOffsetX * (44/86)` and `svgOffsetY = labOffsetY * (43/86)`.
- If source is a spritesheet, crop by slot into per-type SVGs first, then normalize.
  - Use `node screen-record/scripts/generate_cursor_pack.mjs --input <spritesheet.svg> --slug <packslug>` to generate the 12 normalized `cursor-*-<packslug>.svg` files into both `screen-record/public` and `src/overlay/screen_record/dist`.
- Mirror every cursor asset update into both locations:
  - `screen-record/public/...`
  - `src/overlay/screen_record/dist/...`
- New cursor packs must also be wired into the packaged recorder mini app asset server in `src/overlay/screen_record/mod.rs`:
  - add `include_bytes!` constants for each `cursor-*-<slug>.svg`
  - add `path.ends_with("cursor-*-<slug>.svg")` branches in the WebView asset handler
  - otherwise the dev frontend can work while the packaged app shows broken-image placeholders for the new pack
- Apply per-cursor position offsets by editing the SVG content transform (not temporary runtime mapping), so preview/UI/export stay aligned.
- Recorder-side cursor capture can receive non-system/custom cursor handles (won't match IDC_*). Keep fallback mapping resilient so drag cursors still classify as `grab/openhand/closehand` instead of collapsing to default.
- Prefer stable cursor-shape/signature based detection for custom drag cursors and persist learned grab signatures per machine; avoid relying on volatile raw handle values.
- **Adding a new pack** — use `/add-cursor-pack` skill (see `.claude/commands/add-cursor-pack.md` for full workflow)
