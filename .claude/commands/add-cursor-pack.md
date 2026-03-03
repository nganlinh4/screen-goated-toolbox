---
name: add-cursor-pack
description: Wire a new cursor pack into all required app files (video.ts, CursorPanel, cursorTypes, cursorGraphics, cursors.rs, etc.)
allowed-tools: Bash, Read, Edit, Write, Glob, Grep
---

# Add Cursor Pack

Wire a new cursor pack into the screen-goated-toolbox app.

## Inputs

Ask the user for any that weren't provided in the command arguments:

1. **Slug** — lowercase alphanumeric, no hyphens (e.g. `sgtnew`). Must match the filename suffix of the SVG assets.
2. **Display name** — human-readable name shown in the UI (e.g. `"SGT New"`).
3. **Spritesheet path** (optional) — path to the source SVG spritesheet. If omitted, SVG files must already exist in `screen-record/public/`.

## Steps

1. **Confirm inputs** — echo the slug, display name, and spritesheet path back to the user before proceeding.

2. **Run the automation script** (from repo root):
   ```
   node screen-record/scripts/add_cursor_pack.mjs --slug <slug> --name "<name>" [--spritesheet <path>]
   ```
   The script will:
   - Generate per-cursor SVGs from the spritesheet (if provided) via `generate_cursor_pack.mjs`
   - Strip off-screen paths via `clean_svg_viewport.mjs` (spritesheet-split files are ~34KB raw → ~3KB cleaned — **always required**)
   - Patch all 10 source files automatically

3. **Verify TypeScript**:
   ```
   cd screen-record && npx tsc --noEmit
   ```
   Fix any type errors before proceeding.

4. **Verify Rust**:
   ```
   cargo clippy --all-targets
   ```
   Fix any warnings or errors.

5. **Remind the user** to:
   - Open Cursor Lab (`#cursor-lab` hash route) to fine-tune per-cursor offsets
   - Run `node screen-record/scripts/export_cursor_sprite.mjs --slug <slug>` to generate a reference sprite PNG
   - Check one preview frame and one exported frame side-by-side to confirm WYSIWYG

## Key facts

- Slot IDs are auto-detected from `native_export/cursor.rs` — each pack gets 12 contiguous slots
- GPU atlas rows auto-recalculate via `div_ceil` — no shader UV update needed
- Both `screen-record/public/` and `src/overlay/screen_record/dist/` must have the SVG files (the script handles this)
- Cursor Lab offsets are in lab canvas space (86×86), not SVG canvas space (44×43); the conversion is `svgX = labX * (44/86)`
