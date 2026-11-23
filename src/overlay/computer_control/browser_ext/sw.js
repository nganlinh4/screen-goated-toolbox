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
// use it for mutual authentication and durable-key derivation during setup.
// Absent on older installs - then only durable/manual pairing.
try { importScripts("bootstrap.js"); } catch (_) { /* no bootstrap file present */ }
importScripts("frame_registry.js");
importScripts("cdp_admission.js");
importScripts("debugger_revocation.js");
const debuggerRevocations = createDebuggerRevocations(chrome.storage.local);

const DEFAULT_PORT = 47800;
const BRIDGE_PROTOCOL = 11;
const BRIDGE_CAPABILITIES = Object.freeze([
  "cdp.command",
  "cdp.explicit_tab",
  "cdp.session",
  "cdp.dispatch_admission",
  "rpc.effect_admission",
  "rpc.frames",
  "cdp.require_active",
  "tabs.list",
  "tabs.inventory.window_identity",
  "tabs.active",
  "tabs.active.focused_window",
  "tabs.activate",
  "tabs.activate.focus_window",
  "tabs.navigate",
  "tabs.create.foreground",
  "tabs.create.background",
  "tabs.remove",
  "runtime.reload",
  "debugger.user_revocation",
]);
let ws = null;          // the current active socket (for unsolicited events)
let authenticatedChannel = null;
let connecting = false; // guard against overlapping connect() calls
let credentialEpoch = 0;
let backoff = 1000;
let detachTimer = null; // detach the debugger if we stay disconnected
const attached = new Set(); // tabIds we hold a debugger session on
const frameRegistries = new Map();
const attaching = new Map();
const attachmentInterests = new Map();
const detaching = new Map();
const inventoryJobs = new Map();
const MAX_FRAME_ROOTS = 128;
const MAX_CHILD_FRAMES = 128;
const MAX_INVENTORY_JOBS = 32;
const FRAME_SETTLE_TIMEOUT_MS = 2000;
let nextFrameRegistryId = 0;

// Detach chrome.debugger from every tab we hold - so the "being debugged" banner
// clears and DevTools is usable when CC is no longer driving the browser.
async function detachAll() {
  for (const tabId of [...attached]) {
    await detachOwned(tabId, frameRegistries.get(tabId));
  }
}

async function detachOwned(tabId, registry) {
    if (detaching.has(tabId)) { await detaching.get(tabId); return; }
    if (!registry || frameRegistries.get(tabId) !== registry || !attached.has(tabId)) return;
    const operation = (async () => {
      let detached = false;
      try { await chrome.debugger.detach({ tabId }); detached = true; } catch (_) {}
      if (detached && frameRegistries.get(tabId) === registry) {
        attached.delete(tabId);
        frameRegistries.delete(tabId);
      }
      // A rejected detach does not prove Chrome released the debugger.
      if (!detached && frameRegistries.get(tabId) === registry && registry) { registry.needsReconciliation = true; registry.generation++; }
    })();
    detaching.set(tabId, operation);
    try { await operation; } finally { if (detaching.get(tabId) === operation) detaching.delete(tabId); }
}


// A stored port outside 1-65535 would make every WebSocket constructor throw,
// leaving the worker in a silent reconnect loop; fall back to the default.
function validPort(value) {
  return Number.isInteger(value) && value >= 1 && value <= 65535;
}

async function cfg() {
  const o = await chrome.storage.local.get(["port", "secretV8"]);
  return { port: validPort(o.port) ? o.port : DEFAULT_PORT, secret: o.secretV8 || "" };
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

// Only an authenticated, current socket may receive unsolicited events.
function send(obj) {
  if (authenticatedChannel) authenticatedChannel.reply(obj);
}

const effectLedger = createEffectLedger(send);
const reloadHandoff = createReloadHandoff(chrome.storage.local, effectLedger);

function isEffectRequest(body) {
  return Number.isSafeInteger(body.id) && body.id > 0 && body.type !== "frames";
}

function randomNonce() {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function equalMac(a, b) {
  if (!/^[0-9a-f]{64}$/.test(a) || !/^[0-9a-f]{64}$/.test(b)) return false;
  let diff = 0;
  for (let i = 0; i < 64; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

function decodeHex(hex) {
  if (typeof hex !== "string" || !/^(?:[0-9a-f]{2})+$/.test(hex)) throw new Error("invalid hexadecimal bytes");
  return Uint8Array.from(hex.match(/../g), (byte) => parseInt(byte, 16));
}

function encodeHex(bytes) {
  return [...new Uint8Array(bytes)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

function frameNonce(direction, seq) {
  const nonce = new Uint8Array(12);
  nonce[3] = direction === "c2s" ? 1 : 0;
  let remaining = BigInt(seq);
  for (let index = 11; index >= 4; index--) {
    nonce[index] = Number(remaining & 255n);
    remaining >>= 8n;
  }
  return nonce;
}

function channel(sock, clientNonce, secret) {
  let phase = "challenge";
  let key = "";
  let bootstrap = "";
  let keyUse = "";
  let aeadKey = null;
  let serverNonce = "";
  let incoming = 0;
  let outgoing = 0;
  let outbound = Promise.resolve();
  let operations = Promise.resolve();
  const isCurrent = () => ws === sock && sock.readyState === WebSocket.OPEN;
  const close = () => { try { sock.close(); } catch (_) {} };
  const proof = (direction) => `sgt-browser-v8|${direction}|${keyUse}|${clientNonce}|${serverNonce}`;
  const frameAad = (direction, seq) => new TextEncoder().encode(`sgt-browser-v8|frame|${direction}|${clientNonce}|${serverNonce}|${seq}`);
  const reply = (obj) => {
    if (phase !== "ready" || !isCurrent()) return;
    outbound = outbound.then(async () => {
      if (phase !== "ready" || !isCurrent()) return;
      const seq = ++outgoing;
      if (!Number.isSafeInteger(seq)) throw new Error("outbound sequence exhausted");
      const body = new TextEncoder().encode(JSON.stringify(obj));
      const ciphertext = encodeHex(await crypto.subtle.encrypt(
        { name: "AES-GCM", iv: frameNonce("c2s", seq), additionalData: frameAad("c2s", seq), tagLength: 128 },
        aeadKey, body,
      ));
      if (phase === "ready" && isCurrent()) sock.send(JSON.stringify({ type: "secure", seq, ciphertext }));
    }).catch(() => close());
    return outbound;
  };
  const admission = createCdpAdmission(() => phase === "ready" && isCurrent(), reply, effectLedger);
  return {
    reply,
    async receive(raw) {
      if (phase === "closed" || !isCurrent()) return;
      const msg = JSON.parse(raw);
      if (phase === "challenge") {
        if (msg.type !== "challenge" || !["bootstrap", "secret"].includes(msg.use)
          || typeof msg.nonce !== "string" || !/^[0-9a-f]{64}$/.test(msg.nonce)
          || typeof msg.mac !== "string") throw new Error("invalid server challenge");
        keyUse = msg.use;
        bootstrap = keyUse === "bootstrap" ? (self.SGT_BOOTSTRAP_V8 || "") : "";
        key = keyUse === "bootstrap" ? bootstrap : secret;
        if (typeof key !== "string" || !key) throw new Error("missing pairing key");
        if (keyUse === "bootstrap") key = await hmacHex(bootstrap, "sgt-browser-v8|bootstrap-handshake");
        serverNonce = msg.nonce;
        if (!equalMac(msg.mac, await hmacHex(key, proof("server")))) throw new Error("server authentication failed");
        const aeadBytes = decodeHex(await hmacHex(key, `sgt-browser-v8|aead|${clientNonce}|${serverNonce}`));
        aeadKey = await crypto.subtle.importKey("raw", aeadBytes, { name: "AES-GCM" }, false, ["encrypt", "decrypt"]);
        const mac = await hmacHex(key, proof("client"));
        if (!isCurrent()) return;
        phase = keyUse === "bootstrap" ? "pair" : "ready_pending";
        sock.send(JSON.stringify({ type: "auth", mac }));
        return;
      }
      if (msg.type !== "secure" || !Number.isSafeInteger(msg.seq) || msg.seq !== incoming + 1
        || typeof msg.ciphertext !== "string" || !/^(?:[0-9a-f]{2}){16,}$/.test(msg.ciphertext)) {
        throw new Error("invalid authenticated frame");
      }
      const plaintext = await crypto.subtle.decrypt(
        { name: "AES-GCM", iv: frameNonce("s2c", msg.seq), additionalData: frameAad("s2c", msg.seq), tagLength: 128 },
        aeadKey, decodeHex(msg.ciphertext),
      );
      if (!isCurrent()) return;
      incoming = msg.seq;
      const body = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(plaintext));
      if (!body || typeof body !== "object" || Array.isArray(body)) throw new Error("invalid frame body");
      if (phase === "pair") {
        if (body.type !== "pair") throw new Error("expected bootstrap pairing");
        const durable = await hmacHex(bootstrap, "sgt-browser-v8|durable");
        if (!isCurrent()) return;
        await chrome.storage.local.set({ secretV8: durable });
        if (!isCurrent()) return;
        phase = "ready_pending";
        return;
      }
      if (phase === "ready_pending") {
        if (body.type !== "ready") throw new Error("expected authenticated ready");
        await reloadHandoff.ready();
        if (!isCurrent()) return;
        phase = "ready";
        await reply({ type: "ready_ack" });
        if (!isCurrent()) return;
        authenticatedChannel = this;
        await reply(effectLedger.snapshot());
        if (!isCurrent()) return;
        effectLedger.replay();
        backoff = 1000;
        if (detachTimer) { clearTimeout(detachTimer); detachTimer = null; }
        return;
      }
      if (body.type === "request_cancel" || body.type === "cdp_dispatch_grant") {
        admission.control(body);
        return;
      }
      if (body.type === "effect_terminal_ack") {
        if (effectLedger.isTerminal(body.effectToken)) {
          await reloadHandoff.acknowledge(body.effectToken);
          effectLedger.acknowledge(body.effectToken);
        }
        return;
      }
      const effectRequest = isEffectRequest(body);
      const lease = effectRequest ? admission.accept(body) : null;
      if (effectRequest && !lease) {
        reply({ id: body.id, ok: false, error: "invalid or excess effect admission request" });
        return;
      }
      operations = operations.then(async () => {
        if (phase !== "ready" || !isCurrent() || (lease && !lease.valid())) return;
        await onMessage(body, reply, lease);
      }).catch(() => close()).finally(() => { if (lease) admission.finish(lease); });
    },
    close() { phase = "closed"; admission.close(); if (authenticatedChannel === this) authenticatedChannel = null; },
  };
}

async function connect() {
  if (connecting) return;
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;
  connecting = true;
  const epoch = credentialEpoch;
  let port, secret, clientNonce;
  let sock;
  try {
    ({ port, secret } = await cfg());
    if (epoch !== credentialEpoch) {
      connecting = false;
      scheduleReconnect();
      return;
    }
    clientNonce = randomNonce();
    sock = new WebSocket(`ws://127.0.0.1:${port}`);
  } catch (e) {
    connecting = false;
    scheduleReconnect();
    return;
  }
  ws = sock;
  const secure = channel(sock, clientNonce, secret);
  let inbound = Promise.resolve();
  let authTimer = null;
  sock.onopen = () => {
    connecting = false;
    sock.send(JSON.stringify({ type: "hello", bridgeProtocol: BRIDGE_PROTOCOL, clientNonce, capabilities: BRIDGE_CAPABILITIES, extId: chrome.runtime.id, hasSecret: !!secret, hasBootstrap: !!self.SGT_BOOTSTRAP_V8 }));
    authTimer = setTimeout(() => { if (authenticatedChannel !== secure) sock.close(); }, 12000);
  };
  sock.onmessage = (ev) => {
    inbound = inbound.then(() => secure.receive(ev.data)).catch((e) => {
      console.error("[sgt] bridge authentication:", e.message);
      secure.close();
      sock.close();
    });
  };
  sock.onclose = () => {
    secure.close();
    if (authTimer) clearTimeout(authTimer);
    if (ws !== sock) return;
    connecting = false;
    ws = null;
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

async function onMessage(msg, reply, admission) {
  switch (msg.type) {
    case "error":
      console.warn("[sgt] bridge:", msg.error);
      break;
    case "ping":
      reply({ type: "pong" });
      break;
    case "cdp":
      await handleCdp(msg, reply, admission);
      break;
    case "frames":
      await handleFrames(msg, reply);
      break;
    case "tabs":
      await handleTabs(msg, reply, admission);
      break;
    case "runtime":
      await handleRuntime(msg, reply, admission);
      break;
    default:
      if (msg.id) reply({ id: msg.id, ok: false, code: "ERR_BROWSER_CAPABILITY_UNSUPPORTED", capability: `rpc.${msg.type}`, error: "unsupported bridge command" });
      break;
  }
}

async function handleRuntime(msg, reply, admission) {
  if (msg.action !== "reload") {
    reply({ id: msg.id, ok: false, code: "ERR_BROWSER_CAPABILITY_UNSUPPORTED", capability: `runtime.${msg.action || "unknown"}`, error: "unsupported runtime action" });
    return;
  }
  let invoked = false;
  try {
    if (!admission || !admission.valid()) throw new Error("reload expired before preparation");
    await admission.wait(reloadHandoff.prepare(msg.effectToken));
    if (!(await admission.requestGrant())) throw new Error("reload cancelled before admission");
    await admission.invoke(() => {
      invoked = true;
      admission.deferTerminal();
      return chrome.runtime.reload();
    });
  } catch (e) {
    if (!invoked) reply({ id: msg.id, ok: false, error: String(e && e.message ? e.message : e) });
    else console.warn("[sgt] reload invocation failed; awaiting a new worker before resolving its effect");
  } finally {
    if (!invoked) {
      // Storage operations are serialized: a late prepare can only remove its
      // own marker before another reload can reserve the slot.
      void reloadHandoff.abandon(msg.effectToken).catch(() => {});
    }
  }
}

// Resolve the target tab, ensure a debugger session with flat child-target
// attachment, run the CDP command, and reply correlated by id.
async function handleCdp(msg, reply, admission) {
  let attachment;
  try {
    if (!admission || !admission.valid()) throw new Error("CDP request expired before preparation");
    const tabId = msg.tabId || (await admission.wait(activeTabId()));
    if (!tabId) throw new Error("no target tab");
    if (msg.requireActive && tabId !== (await admission.wait(activeTabId()))) {
      throw new Error("target tab is no longer active");
    }
    attachment = await acquireAttachment(tabId, admission.wait, admission.valid);
    if (msg.requireActive && tabId !== (await admission.wait(activeTabId()))) {
      throw new Error("target tab changed before input dispatch");
    }
    if (!(await admission.requestGrant())) throw new Error("CDP dispatch was cancelled before admission");
    if (msg.requireActive && tabId !== (await admission.wait(activeTabId()))) throw new Error("target tab changed before input dispatch");
    if (!admission.valid()) throw new Error("CDP dispatch expired before effect");
    const target = msg.sessionId ? { tabId, sessionId: msg.sessionId } : { tabId };
    const result = await admission.invoke(() => { debuggerRevocations.assertAllowed(tabId); attachment.retain(); return debuggerCommand(target, msg.method, msg.params || {}); });
    reply({ id: msg.id, ok: true, result });
  } catch (e) {
    reply({ id: msg.id, ok: false, code: e?.code, error: String(e && e.message ? e.message : e) });
  } finally {
    if (attachment) attachment.release();
  }
}


// Tab list / activate via chrome.tabs (not a CDP domain).
async function handleTabs(msg, reply, admission) {
  try {
    if (["activate", "navigate", "remove"].includes(msg.action)) await debuggerRevocations.prepare(msg.tabId);
    if (msg.action === "list") {
      if (!admission || !(await admission.requestGrant())) throw new Error("tab list cancelled before admission");
      const tabs = await admission.invoke(() => chrome.tabs.query({}));
      reply({ id: msg.id, ok: true, result: tabs.map((t) => ({
        id: t.id,
        title: t.title,
        url: t.url || null,
        pendingUrl: t.pendingUrl || null,
        active: t.active,
        windowId: t.windowId,
      })) });
    } else if (msg.action === "active") {
      if (!admission || !(await admission.requestGrant())) throw new Error("active tab lookup cancelled before admission");
      const [tab] = await admission.invoke(() => chrome.tabs.query({ active: true, lastFocusedWindow: true }));
      if (!tab || !tab.id) throw new Error("no active tab");
      if (!admission.valid()) throw new Error("active tab lookup expired");
      const browserWindow = await chrome.windows.get(tab.windowId);
      if (!browserWindow || !browserWindow.focused) throw new Error("active browser window is not OS-focused");
      reply({ id: msg.id, ok: true, result: {
        id: tab.id, title: tab.title, url: tab.url, windowId: tab.windowId,
        windowFocused: true,
      } });
    } else if (msg.action === "activate") {
      if (!Number.isInteger(msg.tabId) || msg.tabId <= 0) throw new Error("activate requires a positive tab id");
      if (!admission || !(await admission.requestGrant())) throw new Error("tab activation cancelled before admission");
      const focusWindow = msg.focusWindow === true;
      const windowFocused = await admission.invoke(async () => {
        debuggerRevocations.assertAllowed(msg.tabId);
        const tab = await chrome.tabs.update(msg.tabId, { active: true });
        return focusWindow ? focusTabWindow(tab) : null;
      });
      reply({ id: msg.id, ok: true, result: focusWindow ? { windowFocused } : {} });
    } else if (msg.action === "navigate") {
      if (!Number.isInteger(msg.tabId) || msg.tabId <= 0) throw new Error("navigate requires a positive exact tab id");
      if (typeof msg.url !== "string" || !msg.url) throw new Error("navigate requires a non-empty URL");
      if (!admission || !admission.valid()) throw new Error("navigation expired before preparation");
      const before = await admission.wait(chrome.tabs.get(msg.tabId));
      if (!(await admission.requestGrant())) throw new Error("navigation cancelled before admission");
      const tab = await admission.invoke(() => { debuggerRevocations.assertAllowed(msg.tabId); return chrome.tabs.update(msg.tabId, { url: msg.url }); });
      if (!tab || tab.id !== msg.tabId) throw new Error("exact target tab changed during navigation dispatch");
      reply({ id: msg.id, ok: true, result: {
        id: tab.id,
        beforeUrl: before.url || "",
        url: tab.url || "",
        pendingUrl: tab.pendingUrl || "",
      } });
    } else if (msg.action === "create") {
      if (typeof msg.url !== "string" || !msg.url.trim()) throw new Error("create requires a non-empty URL");
      if (!admission || !admission.valid()) throw new Error("tab creation expired before preparation");
      if (!admission || !(await admission.requestGrant())) throw new Error("tab creation cancelled before admission");
      const tab = await admission.invoke(() => chrome.tabs.create({ url: msg.url, active: msg.active !== false }));
      if (!tab || !Number.isInteger(tab.id) || tab.id <= 0) throw new Error("created tab omitted its id");
      reply({ id: msg.id, ok: true, result: {
        id: tab.id,
        url: tab.url || null,
        pendingUrl: tab.pendingUrl || null,
        requestedUrl: msg.url,
      } });
    } else if (msg.action === "remove") {
      if (!admission || !(await admission.requestGrant())) throw new Error("tab removal cancelled before admission");
      await admission.invoke(() => { debuggerRevocations.assertAllowed(msg.tabId); return chrome.tabs.remove(msg.tabId); });
      reply({ id: msg.id, ok: true, result: {} });
    } else {
      reply({ id: msg.id, ok: false, code: "ERR_BROWSER_CAPABILITY_UNSUPPORTED", capability: `tabs.${msg.action || "unknown"}`, error: "unsupported tabs action" });
    }
  } catch (e) {
    reply({ id: msg.id, ok: false, code: e?.code, error: String(e && e.message ? e.message : e) });
  }
}

// Switching to a tab brings its browser window to the front, as a user's switch
// would. The OS may still refuse foreground; report only what Chrome observes.
async function focusTabWindow(tab) {
  if (!tab || !Number.isInteger(tab.windowId)) return false;
  try {
    await chrome.windows.update(tab.windowId, { focused: true });
    const browserWindow = await chrome.windows.get(tab.windowId);
    return browserWindow?.focused === true;
  } catch (_) {
    return false;
  }
}

async function activeTabId() {
  const [tab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  return tab && tab.id;
}

function debuggerCommand(target, method, params) {
  debuggerRevocations.assertAllowed(target.tabId);
  return chrome.debugger.sendCommand(target, method, params);
}

async function ensureAttached(tabId) {
  await debuggerRevocations.prepare(tabId);
  if (detaching.has(tabId)) await detaching.get(tabId);
  if (attaching.has(tabId)) return attaching.get(tabId);
  const previous = frameRegistries.get(tabId);
  if (attached.has(tabId) && previous && !previous.needsReconciliation && !previous.incomplete) return;
  const operation = (async () => {
    let restorePrevious = false;
    if (attached.has(tabId) && previous?.incomplete) {
      await detachOwned(tabId, previous);
      if (attached.has(tabId) || frameRegistries.get(tabId) === previous) throw new Error("incomplete debugger session could not be rebuilt");
    }
    if (attached.has(tabId) && previous) {
      const generation = previous.generation;
      let stillOwned = false;
      try { await debuggerCommand({ tabId }, "Page.getFrameTree", {}); stillOwned = true; } catch (_) {}
      if (frameRegistries.get(tabId) !== previous || previous.generation !== generation) throw new Error("debugger ownership changed during reconciliation");
      if (stillOwned) { previous.needsReconciliation = false; previous.generation++; return; }
      attached.delete(tabId);
      frameRegistries.delete(tabId);
      restorePrevious = true;
    }
    if (frameRegistries.size >= MAX_FRAME_ROOTS) throw new Error("too many attached browser tabs");
    const registry = { id: ++nextFrameRegistryId, sessions: new Map(), pending: new Set(), generation: 0, incomplete: false, needsReconciliation: false };
    frameRegistries.set(tabId, registry);
    let attachedHere = false;
    try {
      debuggerRevocations.assertAllowed(tabId);
      await attachRootDebugger(tabId, registry, !previous);
      attachedHere = true;
      attached.add(tabId);
      try { debuggerRevocations.assertAllowed(tabId); } catch (error) {
        await detachOwned(tabId, registry);
        throw error;
      }
      const claim = attachmentInterests.get(tabId);
      if (claim) { claim.rootReady = true; claim.registry = registry; scheduleAttachmentCleanup(tabId, claim); }
      await setAutoAttach(tabId, null, registry);
    } catch (error) {
      if (frameRegistries.get(tabId) === registry) {
        if (attachedHere) { registry.incomplete = true; registry.needsReconciliation = true; registry.generation++; }
        else if (restorePrevious) { frameRegistries.set(tabId, previous); attached.add(tabId); previous.needsReconciliation = true; previous.generation++; }
        else frameRegistries.delete(tabId);
      }
      throw error;
    }
  })();
  attaching.set(tabId, operation);
  try { await operation; } finally { if (attaching.get(tabId) === operation) attaching.delete(tabId); }
}

// The host retains at most 16 KiB of each event. Page-controlled payloads (console
// arguments, network bodies) larger than that are never encrypted or sent; the
// host receives the event identity with an explicit omission marker instead.
const MAX_EVENT_PARAMS_CHARS = 16 * 1024;
function boundedDebuggerEvent(source, method, params) {
  const event = { type: "event", tabId: source.tabId, sessionId: source.sessionId, method };
  let serialized;
  try { serialized = JSON.stringify(params ?? {}); } catch (_) { serialized = undefined; }
  if (typeof serialized === "string" && serialized.length <= MAX_EVENT_PARAMS_CHARS) event.params = params;
  else event.event_omitted = true;
  return event;
}
chrome.debugger.onEvent.addListener((source, method, params) => {
  recordFrameEvent(source, method, params);
  send(boundedDebuggerEvent(source, method, params));
});
function onDebuggerDetached(source, reason) {
  if (reason === "canceled_by_user") void debuggerRevocations.revoke(source.tabId).catch(() => {});
  if (source.tabId) {
    const registry = frameRegistries.get(source.tabId);
    // onDetach has no attachment generation. Even during our own detach it
    // might describe an older session, so only the detach promise may erase
    // the registry it owns; notifications require a fresh ownership probe.
    if (registry) { registry.needsReconciliation = true; registry.generation++; }
    else attached.delete(source.tabId);
  }
  send({ type: "event", tabId: source.tabId, method: "SGT.debuggerDetached", params: { reason } });
}
chrome.debugger.onDetach.addListener(onDebuggerDetached);
chrome.tabs.onRemoved.addListener((tabId) => {
  attached.delete(tabId); frameRegistries.delete(tabId);
  void debuggerRevocations.forget(tabId).catch(() => {});
});

setInterval(() => send({ type: "keepalive" }), 20000);
chrome.runtime.onStartup.addListener(() => {
  void debuggerRevocations.retirePreviousBrowserSession().catch(() => {});
  connect();
});
chrome.runtime.onInstalled.addListener(connect);
chrome.runtime.onMessage.addListener((m, sender, respond) => {
  if (m?.type === "resume-debugging" && sender?.id === chrome.runtime.id && sender?.url === chrome.runtime.getURL("popup.html")) {
    void debuggerRevocations.resume(m.tabId).then(() => respond({ ok: true }), error => respond({ ok: false, error: error.message }));
    return true;
  }
  if (m !== "reconnect") return;
  credentialEpoch++;
  if (ws) {
    try { ws.close(); } catch (_) {}
  } else if (!connecting) {
    connect();
  }
});
connect();
