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
- `bootstrap.js` — generated per install: the one-time bootstrap key the app stamps
  in so the extension can prove itself on first connect (loaded via `importScripts`).
- `popup.html` / `popup.js` — status view + a manual-pair / Forget debug fallback.

## Install (dev / load-unpacked)
The app's agent does this for you via `browser_setup` — **no code to paste**:
1. The app extracts these files to `%LOCALAPPDATA%/screen-goated-toolbox/cc_browser_ext`.
2. `chrome://extensions` → enable **Developer mode** → **Load unpacked** → that folder.
3. Approve the permission prompt if one appears (it can read/change browser data).
4. It **auto-pairs** over the socket within ~2 minutes — the extension proves the
   stamped bootstrap key and the app hands back the durable secret. (The popup's
   manual paste stays only as a debug fallback.)

## Security
- Connects only to `ws://127.0.0.1:<port>` (default 47800); web-page Origins rejected.
- **First connect:** the extension proves the per-install **bootstrap key** (stamped
  into its own files, so a random local socket client that can't read them can't
  pair) → the app hands over the durable secret. The secret is never sent to an
  unauthenticated socket and is not surfaced in `browser_status`.
- **Every reconnect:** **HMAC challenge-response** over the durable secret — no
  static token on the wire. **Forget** wipes it.
- The persistent "being debugged" banner is the honest "automation active" signal.

## Packaging
`pwsh scripts/pack-browser-ext.ps1` → a Web Store zip (one-click install + a
store-assigned stable ID).
