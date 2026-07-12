# SGT Browser Use Extension

Manifest V3 bridge between Computer Control and the user's existing Chromium session. The extension exposes Chrome DevTools Protocol over a paired loopback WebSocket; planning, policy, and tool execution remain in Rust.

## Components

- `manifest.json` — extension metadata and `debugger`, `tabs`, `storage` permissions.
- `sw.js` — WebSocket owner, HMAC authentication, CDP sessions, frame targets, and tab operations.
- `bootstrap.js` — generated per extraction with a one-time bootstrap credential.
- `popup.html` / `popup.js` — status, manual pairing fallback, and Forget action.

## Development install

`browser_setup` extracts the packaged extension to:

`%APPDATA%\screen-goated-toolbox\cc_browser_ext`

Then:

1. Open the browser extension manager.
2. Enable developer mode.
3. Load the extracted directory as an unpacked extension.
4. If it was already loaded, press Reload once after an app update so staged protocol code takes effect.

The app opens a ten-minute setup pairing window. A newly loaded/reloaded extension connects and pairs automatically while that window is open; the popup supports manual recovery.

## Security model

- Network scope is loopback only: `ws://127.0.0.1:<port>`; default port is `47800`.
- Browser-page origins are rejected by the Rust bridge.
- First pair proves the generated bootstrap credential before receiving a durable secret.
- Reconnects use HMAC challenge-response; the durable secret is not sent as a static wire token.
- Forget removes the stored secret.
- Chrome's debugging banner remains visible while CDP is attached.
- Protocol version is shared by `BRIDGE_PROTOCOL` in `sw.js` and the Rust setup/status checks.

## Packaging

The desktop binary embeds this directory and generates `bootstrap.js` only during extraction. The packaging helper can create a source zip:

```powershell
.\scripts\pack-browser-ext.ps1
```

Output: `target/dist/sgt-browser-bridge-<manifest-version>.zip`.

That zip does not contain the generated bootstrap credential. A fresh store-installed copy therefore cannot complete first-pair authentication through the current flow. Treat the zip as a development artifact, not a publish-ready Web Store package, until store-install pairing is implemented.
