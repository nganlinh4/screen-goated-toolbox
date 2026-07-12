# PromptDJ Mini App

Vite frontend for SGT's embedded PromptDJ/MIDI experience. Rust hosts the built assets from `src/overlay/prompt_dj/dist/` and injects application settings/API credentials at runtime.

## Standalone development

```powershell
cd promptdj-midi
npm install
$env:GEMINI_API_KEY = '<development key>'
npm run dev
```

The Vite config also reads `GEMINI_API_KEY` from a local Vite environment file when present. Never commit credentials.

## Build

```powershell
npm run build
```

Root `run-dev.ps1` builds and copies `dist/` into `src/overlay/prompt_dj/dist/`. Use that path to test the actual embedded WebView host rather than only the standalone Vite page.
