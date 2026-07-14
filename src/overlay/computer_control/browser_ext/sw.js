// SGT Browser Use — service worker.
//
// Owns BOTH the WebSocket to the SGT app AND chrome.debugger (CDP). The SW is
// kept alive by WebSocket traffic (Chrome 116+) and during an active debugger
// session (118+); a 20s keepalive ping covers idle gaps.
//
// IMPORTANT: every connection sends/replies on ITS OWN socket (captured in the
// closure), never the global `ws`. connect() can fire from several triggers, and
// using the global led to onopen sending `hello` on the wrong (newer) socket, so
// the server's accepted socket never got it → "timed out waiting for hello" loop.

// The per-install bootstrap key SGT stamped into bootstrap.js at extract time. We
// prove knowledge of it on the FIRST connect to receive the durable secret (then it
// is never used again). Absent on older installs - then only durable/manual pairing.
try { importScripts("bootstrap.js"); } catch (_) { /* no bootstrap file present */ }

const DEFAULT_PORT = 47800;
const BRIDGE_PROTOCOL = 5;
const BRIDGE_CAPABILITIES = Object.freeze([
  "cdp.command",
  "cdp.explicit_tab",
  "cdp.session",
  "cdp.require_active",
  "tabs.list",
  "tabs.active",
  "tabs.active.focused_window",
  "tabs.activate",
  "tabs.navigate",
  "tabs.create.foreground",
  "tabs.create.background",
  "tabs.remove",
]);
let ws = null;          // the current active socket (for unsolicited events)
let connecting = false; // guard against overlapping connect() calls
let backoff = 1000;
let detachTimer = null; // detach the debugger if we stay disconnected
const attached = new Set(); // tabIds we hold a debugger session on

// Detach chrome.debugger from every tab we hold - so the "being debugged" banner
// clears and DevTools is usable when CC is no longer driving the browser.
async function detachAll() {
  for (const tabId of [...attached]) {
    try { await chrome.debugger.detach({ tabId }); } catch (_) {}
    attached.delete(tabId);
  }
}

async function cfg() {
  const o = await chrome.storage.local.get(["port", "secret"]);
  return { port: o.port || DEFAULT_PORT, secret: o.secret || "" };
}

// HMAC-SHA256(secret, msg) as lowercase hex — the pairing challenge-response.
async function hmacHex(secret, msg) {
  const enc = new TextEncoder();
  const key = await crypto.subtle.importKey(
    "raw", enc.encode(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]
  );
  const sig = await crypto.subtle.sign("HMAC", key, enc.encode(msg));
  return [...new Uint8Array(sig)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

// Send on the CURRENTLY active socket (for events / keepalive only).
function send(obj) {
  if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(obj));
}

async function connect() {
  if (connecting) return;
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;
  connecting = true;
  const { port, secret } = await cfg();
  let sock;
  try {
    sock = new WebSocket(`ws://127.0.0.1:${port}`);
  } catch (e) {
    connecting = false;
    scheduleReconnect();
    return;
  }
  ws = sock;
  // Reply ON THIS socket — never the global, which may have been replaced.
  const reply = (obj) => {
    if (sock.readyState === WebSocket.OPEN) sock.send(JSON.stringify(obj));
  };
  sock.onopen = () => {
    connecting = false;
    backoff = 1000;
    if (detachTimer) { clearTimeout(detachTimer); detachTimer = null; } // reconnected in time
    reply({ type: "hello", bridgeProtocol: BRIDGE_PROTOCOL, capabilities: BRIDGE_CAPABILITIES, extId: chrome.runtime.id, hasSecret: !!secret, hasBootstrap: !!self.SGT_BOOTSTRAP });
  };
  sock.onmessage = (ev) => onMessage(ev.data, reply).catch((e) => console.error("[sgt]", e));
  sock.onclose = () => {
    connecting = false;
    if (ws === sock) ws = null;
    // If we don't reconnect within the grace period, release the debugger.
    if (!detachTimer) detachTimer = setTimeout(() => { detachTimer = null; detachAll(); }, 15000);
    scheduleReconnect();
  };
  sock.onerror = () => {
    try { sock.close(); } catch (_) {}
  };
}

function scheduleReconnect() {
  backoff = Math.min(backoff * 2, 15000);
  setTimeout(connect, backoff);
}

async function onMessage(raw, reply) {
  const msg = JSON.parse(raw);
  switch (msg.type) {
    case "challenge": {
      // The app picks which key to prove: the per-install bootstrap key on the
      // first connect, else the durable secret. Both HMAC over the server nonce.
      const { secret } = await cfg();
      const key = msg.use === "bootstrap" ? (self.SGT_BOOTSTRAP || "") : secret;
      reply({ type: "auth", extId: chrome.runtime.id, mac: await hmacHex(key, msg.nonce) });
      break;
    }
    case "pair":
      // Trust-on-first-use: the app handed us the secret during its pairing
      // window. Store it; future connections use HMAC challenge-response.
      await chrome.storage.local.set({ secret: msg.secret });
      break;
    case "error":
      console.warn("[sgt] bridge:", msg.error);
      break;
    case "ping":
      reply({ type: "pong" });
      break;
    case "cdp":
      await handleCdp(msg, reply);
      break;
    case "tabs":
      await handleTabs(msg, reply);
      break;
    default:
      if (msg.id) reply({ id: msg.id, ok: false, code: "ERR_BROWSER_CAPABILITY_UNSUPPORTED", capability: `rpc.${msg.type}`, error: "unsupported bridge command" });
      break;
  }
}

// Resolve the target tab, ensure a debugger session with flat child-target
// attachment, run the CDP command, and reply correlated by id.
async function handleCdp(msg, reply) {
  try {
    const tabId = msg.tabId || (await activeTabId());
    if (!tabId) throw new Error("no target tab");
    if (msg.requireActive && tabId !== (await activeTabId())) {
      throw new Error("target tab is no longer active");
    }
    await ensureAttached(tabId);
    if (msg.requireActive && tabId !== (await activeTabId())) {
      throw new Error("target tab changed before input dispatch");
    }
    const target = msg.sessionId ? { sessionId: msg.sessionId } : { tabId };
    const result = await chrome.debugger.sendCommand(target, msg.method, msg.params || {});
    reply({ id: msg.id, ok: true, result });
  } catch (e) {
    reply({ id: msg.id, ok: false, error: String(e && e.message ? e.message : e) });
  }
}

// Tab list / activate via chrome.tabs (not a CDP domain).
async function handleTabs(msg, reply) {
  try {
    if (msg.action === "list") {
      const tabs = await chrome.tabs.query({});
      reply({ id: msg.id, ok: true, result: tabs.map((t) => ({ id: t.id, title: t.title, url: t.url, active: t.active })) });
    } else if (msg.action === "active") {
      const [tab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
      if (!tab || !tab.id) throw new Error("no active tab");
      const browserWindow = await chrome.windows.get(tab.windowId);
      if (!browserWindow || !browserWindow.focused) throw new Error("active browser window is not OS-focused");
      reply({ id: msg.id, ok: true, result: {
        id: tab.id, title: tab.title, url: tab.url, windowId: tab.windowId,
        windowFocused: true,
      } });
    } else if (msg.action === "activate") {
      await chrome.tabs.update(msg.tabId, { active: true });
      reply({ id: msg.id, ok: true, result: {} });
    } else if (msg.action === "navigate") {
      if (!Number.isInteger(msg.tabId)) throw new Error("navigate requires an exact tab id");
      if (typeof msg.url !== "string" || !msg.url) throw new Error("navigate requires a non-empty URL");
      const before = await chrome.tabs.get(msg.tabId);
      const tab = await chrome.tabs.update(msg.tabId, { url: msg.url });
      if (!tab || tab.id !== msg.tabId) throw new Error("exact target tab changed during navigation dispatch");
      reply({ id: msg.id, ok: true, result: {
        id: tab.id,
        beforeUrl: before.url || "",
        url: tab.url || "",
        pendingUrl: tab.pendingUrl || "",
      } });
    } else if (msg.action === "create") {
      const tab = await chrome.tabs.create({ url: msg.url, active: msg.active !== false });
      reply({ id: msg.id, ok: true, result: { id: tab.id, url: tab.url } });
    } else if (msg.action === "remove") {
      await chrome.tabs.remove(msg.tabId);
      reply({ id: msg.id, ok: true, result: {} });
    } else {
      reply({ id: msg.id, ok: false, code: "ERR_BROWSER_CAPABILITY_UNSUPPORTED", capability: `tabs.${msg.action || "unknown"}`, error: "unsupported tabs action" });
    }
  } catch (e) {
    reply({ id: msg.id, ok: false, error: String(e && e.message ? e.message : e) });
  }
}

async function activeTabId() {
  const [tab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  return tab && tab.id;
}

async function ensureAttached(tabId) {
  if (attached.has(tabId)) return;
  await chrome.debugger.attach({ tabId }, "1.3");
  attached.add(tabId);
  try {
    await chrome.debugger.sendCommand({ tabId }, "Target.setAutoAttach", {
      autoAttach: true, waitForDebuggerOnStart: false, flatten: true,
    });
  } catch (_) {}
}

chrome.debugger.onEvent.addListener((source, method, params) => {
  send({ type: "event", tabId: source.tabId, sessionId: source.sessionId, method, params });
});
chrome.debugger.onDetach.addListener((source, reason) => {
  if (source.tabId) attached.delete(source.tabId);
  send({ type: "detached", tabId: source.tabId, reason });
});
chrome.tabs.onRemoved.addListener((tabId) => attached.delete(tabId));

setInterval(() => send({ type: "keepalive" }), 20000);
chrome.runtime.onStartup.addListener(connect);
chrome.runtime.onInstalled.addListener(connect);
chrome.runtime.onMessage.addListener((m) => { if (m === "reconnect") connect(); });
connect();
