# SGT Browser Use (Chrome extension)

A thin **CDP-over-WebSocket bridge**: it attaches `chrome.debugger` to the active
tab and forwards DevTools Protocol commands to the Screen Goated Toolbox app over a
local, paired WebSocket. All the agent logic lives in the Rust app — this is just
the gateway that lets it act in your **real, logged-in** browser session.

See `../../../../temp-browser-extension-design.md` for the full design.

## Files
- `manifest.json` — MV3; permissions kept minimal (`debugger`, `tabs`, `storage`).
- `sw.js` — service worker: owns the WS (+ ≤20s keepalive) and `chrome.debugger`;
  HMAC challenge-response pairing; flat sessions for child frames/OOPIFs.
- `popup.html` / `popup.js` — one-time pairing (paste the code) + status + Forget.

## Install (dev / load-unpacked)
The app's agent does this for you via `browser_setup`, but manually:
1. The app extracts these files to `%LOCALAPPDATA%/screen-goated-toolbox/cc_browser_ext`.
2. `chrome://extensions` → enable **Developer mode** → **Load unpacked** → that folder.
3. Approve the permission prompt (it can read/change browser data).
4. Click the extension icon → paste the **pairing code** the app shows → **Pair & connect**.

## Security
- Connects only to `ws://127.0.0.1:<port>` (default 47800).
- Authenticates every connection with **HMAC challenge-response** over a shared
  secret (the pairing code) — no static token on the wire. **Forget** wipes it.
- The persistent "being debugged" banner is the honest "automation active" signal.

## Packaging
`pwsh scripts/pack-browser-ext.ps1` → a Web Store zip (one-click install + a
store-assigned stable ID).
