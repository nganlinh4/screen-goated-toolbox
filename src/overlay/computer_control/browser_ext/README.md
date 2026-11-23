# SGT Browser Use Extension

Manifest V3 bridge between Computer Control and the user's existing Chromium session. The extension exposes Chrome DevTools Protocol over a paired loopback WebSocket; planning, policy, and tool execution remain in Rust.

## Components

- `manifest.json` — extension metadata and permissions for debugger/tab control
  and paired storage.
- `sw.js` — WebSocket owner, HMAC authentication, CDP sessions, and tab operations.
- `frame_registry.js` — bounded frame-session inventory and coverage checks.
- `cdp_admission.js` — revocable effect admission and reconnectable terminal evidence.
- `debugger_revocation.js` — persisted per-tab user revocations and popup-only resumption.
- `bootstrap.js` — generated per extraction with the per-install bootstrap credential.
- `popup.html` / `popup.js` — status, manual pairing fallback, and Forget action.

## Development install

`browser_setup` extracts the packaged extension to:

`%APPDATA%\screen-goated-toolbox\cc_browser_ext`

Then:

1. Open the browser extension manager.
2. Enable developer mode.
3. Load the extracted directory as an unpacked extension.
4. Existing installations below protocol 8 require one manual Reload to activate the authenticated transport. Later compatible updates can use the bounded self-reload handshake.

The app opens a ten-minute setup pairing window. A newly loaded/reloaded extension connects and pairs automatically while that window is open; the popup supports manual recovery.

## Security model

- Network scope is loopback only: `ws://127.0.0.1:<port>`; default port is `47800`.
- A present Origin must be a valid Chromium extension origin; after pairing,
  it is pinned outside a new setup window. Opaque, malformed, and browser-page
  origins are rejected. A missing Origin remains compatible with service
  workers and still requires mutual authentication.
- First pair authenticates both peers with the generated bootstrap credential and derives a durable key locally. Reconnects authenticate both peers with the durable key.
- Commands, responses, and events use session-bound authenticated encryption and ordered sequence numbers. Pairing keys remain local.
- Pairing material is read and created only in the active runtime's config root.
  Existing invalid or inaccessible material is preserved and reported through
  `browser_status.pairing_key_error`; setup fails with the corresponding typed
  error before extracting files. New keys are published without overwriting a
  concurrent creator. After resolving a storage or entropy failure, restart the
  application before retrying setup.
- Forget removes the stored secret.
- Chrome's debugging banner remains visible while CDP is attached.
- Cancelling Chrome's debugging banner pauses debugger commands and tab mutations
  for that tab. Reconnecting or reloading the extension does not clear the saved
  revocation. To allow control again, open the SGT extension popup for the tab
  and click **Resume control for this tab**. Commands report
  `ERR_BROWSER_DEBUGGER_REVOKED`; failed or malformed revocation storage pauses
  control with `ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE`.
- Revocations are bounded to 256 tab IDs. Closing a tab retires its record when
  the extension observes the close. Chrome tab IDs are unique only within one
  browser session, so a browser start retires the records it restored; a
  Cancel made in the new session is kept.
- The connection hello advertises protocol and per-command capabilities. Rust keeps
  conservative maps before the capability-schema boundary and honors authenticated
  advertised capabilities after it, so optional staged updates do not disable
  commands the connected extension already supports.
- A staged update can request `runtime.reload` only when the connected worker
  explicitly advertised that capability. Requests are version-bound and cooldown
  limited; older workers continue to require the manual legacy reload.

## Control behavior

- `browser_switch_tab` activates the tab and, with `tabs.activate.focus_window`,
  brings its browser window to the front, as a user's switch would. Windows can
  refuse foreground; the receipt then reports `window_focused: false` instead of
  claiming focus. Restoring a temporary tab's predecessor never moves focus.
- Frame coverage is all-or-nothing and bounded to 128 child frames per tab. A
  larger page is refused with `ERR_BROWSER_TOO_MANY_FRAMES` (not retryable), so
  the agent uses screen-level tools for it instead of acting on partial coverage.

## Packaging

The desktop binary embeds this directory and generates `bootstrap.js` only during extraction. The packaging helper can create a source zip:

```powershell
.\scripts\pack-browser-ext.ps1
```

Output: `target/dist/sgt-browser-bridge-<manifest-version>.zip`.

That zip does not contain the generated bootstrap credential. A fresh store-installed copy therefore cannot complete first-pair authentication through the current flow. Treat the zip as a development artifact, not a publish-ready Web Store package, until store-install pairing is implemented.
