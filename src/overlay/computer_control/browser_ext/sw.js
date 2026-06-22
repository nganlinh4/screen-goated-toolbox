// SGT Computer Control Bridge — service worker.
//
// Owns BOTH the WebSocket to the SGT app AND chrome.debugger (CDP). The SW is
// kept alive by WebSocket traffic (Chrome 116+) and during an active debugger
// session (118+); a 20s keepalive ping covers idle gaps.
//
// IMPORTANT: every connection sends/replies on ITS OWN socket (captured in the
// closure), never the global `ws`. connect() can fire from several triggers, and
// using the global led to onopen sending `hello` on the wrong (newer) socket, so
// the server's accepted socket never got it → "timed out waiting for hello" loop.

const DEFAULT_PORT = 47800;
let ws = null;          // the current active socket (for unsolicited events)
let connecting = false; // guard against overlapping connect() calls
let backoff = 1000;
const attached = new Set(); // tabIds we hold a debugger session on

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
    reply({ type: "hello", extId: chrome.runtime.id, hasSecret: !!secret });
  };
  sock.onmessage = (ev) => onMessage(ev.data, reply).catch((e) => console.error("[sgt]", e));
  sock.onclose = () => {
    connecting = false;
    if (ws === sock) ws = null;
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
      const { secret } = await cfg();
      reply({ type: "auth", extId: chrome.runtime.id, mac: await hmacHex(secret, msg.nonce) });
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
      break;
  }
}

// Resolve the target tab, ensure a debugger session with flat child-target
// attachment, run the CDP command, and reply correlated by id.
async function handleCdp(msg, reply) {
  try {
    const tabId = msg.tabId || (await activeTabId());
    if (!tabId) throw new Error("no target tab");
    await ensureAttached(tabId);
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
    } else if (msg.action === "activate") {
      await chrome.tabs.update(msg.tabId, { active: true });
      reply({ id: msg.id, ok: true, result: {} });
    } else if (msg.action === "create") {
      const tab = await chrome.tabs.create({ url: msg.url, active: true });
      reply({ id: msg.id, ok: true, result: { id: tab.id, url: tab.url } });
    } else {
      reply({ id: msg.id, ok: false, error: "bad tabs action" });
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
