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

The `tsc` build lane is native TypeScript 7. `typescript-eslint` still consumes
the legacy compiler API, so the canonical `typescript` dependency is the
official TypeScript 6 compatibility package while `@typescript/native` owns the
`tsc` executable. Do not collapse the two dependencies until the linter supports
the TypeScript 7 API.

Root `run-dev.ps1` builds and copies `dist/` into `src/overlay/screen_record/dist/` before running Rust.

## Validation

Fast default validation runs the complete Vitest suite once and produces the
frontend assets:

```powershell
npm run lint
npm test
npm run build
```

`test:unit` and `test:components` are focused alternatives to `npm test`; do
not run them again after the complete suite. Add only the affected boundary
gate while iterating:

```powershell
# Browser interaction or editor workflow changes
npm run test:e2e

# Rust host, WebView2, IPC, packaged routes, or font changes
npm run test:wry

# Timeline, renderer, history, media import, or large-project hot paths
npm run test:perf
```

The Wry command launches an isolated test instance and exercises the real
WebView2 shell through Playwright. It supersedes the old page-presence smoke
test, which force-terminated every running SGT process. Run all affected gates
before packaging the recorder; routine UI changes do not require unrelated
Wry or performance suites.

## Architecture map

- `src/App.tsx` — editor composition and top-level state wiring.
- `src/hooks/useProjectEditorState.ts` — canonical active-project identity and editor state refs.
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
Vite-generated direct-child JavaScript and CSS chunks under `assets/` are supported by the packaged route; other extensions and nested paths remain denied.

### UI code

- Add semantic kebab-case class names to JSX elements.
- Reuse shared surface/button/timeline primitives before adding one-off CSS.
- Keep pointer interactions on pointer events.
- Clip inner visual wrappers when handles must extend outside rounded tracks.

### Shared background catalog

Built-in data, `defaultId`, and `panelOrder` live in `src/config/shared-background-presets.json`. Preview families are implemented in `src/lib/renderer/builtInBackgrounds.ts`; Rust consumes the same catalog through `src/overlay/screen_record/native_export/background_presets.rs` and the GPU export path.

### Retention and ownership

Recorder projects use the persisted 10–100 project limit (default 50). Recent
uploaded backgrounds use the persisted 4–24 limit (default 12). Reducing either
limit must prune its backing storage, not only the visible cards. An uploaded
background file may be deleted only after reference-scanning the active editor,
every retained project, and every composition clip. Recordings and exports are
user output and are never removed by automatic retention or Downloaded Tools.

## Related guidance

- UI workflow: `../.claude/skills/update-frontend/SKILL.md`
- Background workflow: `../.claude/commands/manage-background-presets.md`
- Preview/export math: `docs/render-parity.md`
- Root development commands: `../docs/DEVELOPMENT.md`
