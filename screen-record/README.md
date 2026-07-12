# SGT Screen Recorder Frontend

React/Vite editor embedded by the Windows Rust application through WebView2/Wry. This directory owns recorder UI, preview rendering, timeline editing, and frontend-side export state. Native capture/export lives under `src/overlay/screen_record/`.

This is not a standalone Tauri application and has no separate release channel.

## Development

```powershell
cd screen-record
npm install
npm run dev
```

Production asset build:

```powershell
npm run build
```

Root `run-dev.ps1` builds and copies `dist/` into `src/overlay/screen_record/dist/` before running Rust.

## Validation

```powershell
npm test
npm run test:unit
npm run test:components
npm run test:e2e
npm run test:wry
npm run test:wry:playwright
npm run test:perf
npm run build
```

Use focused suites while iterating; run affected integration/Wry tests before shipping cross-boundary changes.

## Architecture map

- `src/App.tsx` — editor composition and top-level state wiring.
- `src/App.css` — global tokens and shared visual primitives.
- `src/components/VideoPreview.tsx` — preview surface and playback controls.
- `src/components/timeline/` — trim, camera, speed, audio, text, subtitle, pointer, and narration tracks.
- `src/components/sidepanel/` — feature configuration panels.
- `src/components/dialogs/` — export, media result, and selection dialogs.
- `src/lib/renderer/` — preview renderer and cursor/background composition.
- `src/lib/videoRenderer.ts` — frontend export/render coordination.
- `src/config/shared-background-presets.json` — canonical built-in background data/order/default.
- `tests/` — unit, component, E2E, Wry, and performance coverage.
- `../src/overlay/screen_record/` — Rust host, capture, packaged assets, native export, and GPU shaders.

## Contracts

### Preview equals export

Preview and exported media must consume the same state and parameter model. Do not tune a separate export look. For changed backgrounds, cursors, camera math, text, subtitles, or effects, compare preview and export at the same timestamp.

### Packaged assets

A Vite dev page working does not prove the desktop app works. New static assets must also reach `src/overlay/screen_record/dist/` and the Rust packaged asset route where applicable.

### UI code

- Add semantic kebab-case class names to JSX elements.
- Reuse shared surface/button/timeline primitives before adding one-off CSS.
- Keep pointer interactions on pointer events.
- Clip inner visual wrappers when handles must extend outside rounded tracks.

### Shared background catalog

Built-in data, `defaultId`, and `panelOrder` live in `src/config/shared-background-presets.json`. Preview families are implemented in `src/lib/renderer/builtInBackgrounds.ts`; Rust consumes the same catalog through `src/overlay/screen_record/native_export/background_presets.rs` and the GPU export path.

## Related guidance

- UI workflow: `../.claude/skills/update-frontend/SKILL.md`
- Background workflow: `../.claude/commands/manage-background-presets.md`
- Preview/export math: `docs/render-parity.md`
- Root development commands: `../docs/DEVELOPMENT.md`
