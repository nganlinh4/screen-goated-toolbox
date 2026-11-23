const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createHmac, createCipheriv, createDecipheriv, webcrypto } = require('node:crypto');

const source = fs.readFileSync(path.join(__dirname, 'sw.js'), 'utf8');
const registrySource = fs.readFileSync(path.join(__dirname, 'frame_registry.js'), 'utf8');
const admissionSource = fs.readFileSync(path.join(__dirname, 'cdp_admission.js'), 'utf8');
const revocationSource = fs.readFileSync(path.join(__dirname, 'debugger_revocation.js'), 'utf8');
const hmac = (key, message) => createHmac('sha256', key).update(message).digest('hex');
const listeners = () => ({ addListener(fn) { this.handler = fn; } });
const sleep = () => new Promise((resolve) => setTimeout(resolve, 2));
async function until(predicate) {
  for (let i = 0; i < 1000; i++) { if (predicate()) return; await sleep(); }
  assert.fail('timed out waiting for worker state');
}

async function harness({ bootstrap = '', secret = '', legacyBootstrap = '', legacySecret = '', storageError = false, attachHook = null, queryHook = null, commandHook = null, sharedStored = null, setHook = null, reloadHook = null, windowFocusGranted = true } = {}) {
  const sockets = [], operations = [], timers = [], stored = sharedStored || { secretV8: secret, secret: legacySecret };
  const sessionStored = {};
  const debuggerEvents = listeners(), debuggerDetach = listeners(), tabUpdated = listeners(), tabRemoved = listeners();
  const startup = listeners(), installed = listeners(), messages = listeners();
  class Socket {
    static OPEN = 1;
    static CONNECTING = 0;
    constructor(url) { this.url = url; this.readyState = 0; this.sent = []; this.snapshots = []; this.secretAtSend = []; sockets.push(this); }
    open() { this.readyState = 1; this.onopen(); }
    send(value) {
      const parsed = JSON.parse(value);
      if (parsed.type === 'secure' && this.challenge) {
        const key = this.challenge.use === 'bootstrap' ? hmac(bootstrap, 'sgt-browser-v8|bootstrap-handshake') : stored.secretV8;
        const state = { clientNonce: this.sent[0].clientNonce, serverNonce: this.challenge.nonce };
        if (decrypt(key, state, parsed).type === 'effect_ledger_snapshot') { this.snapshots.push(decrypt(key, state, parsed)); return; }
      }
      this.sent.push(parsed);
      this.secretAtSend.push(stored.secretV8);
    }
    receive(value) { if (value.type === 'challenge') this.challenge = value; this.onmessage({ data: JSON.stringify(value) }); }
    close() { if (this.readyState === 3) return; this.readyState = 3; this.onclose?.(); }
  }
  const chrome = {
    storage: { session: {
      async get() { return { ...sessionStored }; },
      async set(values) { Object.assign(sessionStored, values); },
    }, local: {
      async get() { return { ...stored }; },
      async set(values) { operations.push(['storage.set', values]); if (setHook) await setHook(values); if (storageError) throw new Error('storage failed'); Object.assign(stored, values); },
      async remove(name) { delete stored[name]; },
    } },
    runtime: { id: 'extension-id', getURL: name => `chrome-extension://extension-id/${name}`, onStartup: startup, onInstalled: installed, onMessage: messages, reload() { operations.push(['reload']); if (reloadHook) reloadHook(); } },
    debugger: {
      onEvent: debuggerEvents, onDetach: debuggerDetach,
      async attach(target) { operations.push(['attach', target]); if (attachHook) await attachHook(); },
      async detach(target) { operations.push(['detach', target]); },
      async sendCommand(target, method) { operations.push(['cdp', target, method]); return commandHook ? await commandHook(target, method) : {}; },
    },
    tabs: {
      onRemoved: tabRemoved, onUpdated: tabUpdated,
      async query() { operations.push(['tabs.query']); return queryHook ? await queryHook() : []; },
      async create(value) { operations.push(['tabs.create', value]); return { id: 33 }; },
      async update(id, value) { operations.push(['tabs.update', id, value]); return { id, windowId: 5 }; },
      async remove(id) { operations.push(['tabs.remove', id]); },
      async get(id) { operations.push(['tabs.get', id]); return { id, url: 'https://example.com/' }; },
    },
    windows: {
      async get(id) { return { id, focused: windowFocusGranted || !operations.some(op => op[0] === 'windows.update') }; },
      async update(id, value) { operations.push(['windows.update', id, value]); return { id }; },
    },
  };
  const context = {
    chrome, crypto: webcrypto, TextEncoder, TextDecoder, Uint8Array, WebSocket: Socket,
    console: { error() {}, warn() {} },
    setTimeout(fn, ms) { const timer = { fn, ms, cleared: false }; timers.push(timer); return timer; },
    clearTimeout(timer) { timer.cleared = true; }, setInterval() {},
    importScripts(name) { if (name === 'frame_registry.js') vm.runInContext(registrySource, context); if (name === 'cdp_admission.js') vm.runInContext(admissionSource, context); if (name === 'debugger_revocation.js') vm.runInContext(revocationSource, context); }, self: { SGT_BOOTSTRAP_V8: bootstrap, SGT_BOOTSTRAP: legacyBootstrap },
    URL,
    performance: require('node:perf_hooks').performance,
  };
  vm.createContext(context);
  vm.runInContext(source, context, { filename: 'sw.js' });
  await until(() => sockets.length === 1);
  const sock = sockets[0]; sock.open();
  return { sock, sockets, operations, stored, debuggerEvents, debuggerDetach, tabUpdated, tabRemoved, startup, messages, timers };
}

function challenge(sock, key, mode, serverNonce = 'ab'.repeat(32), overrideMac) {
  const clientNonce = sock.sent[0].clientNonce;
  const proof = `sgt-browser-v8|server|${mode}|${clientNonce}|${serverNonce}`;
  const proofKey = mode === 'bootstrap' ? hmac(key, 'sgt-browser-v8|bootstrap-handshake') : key;
  sock.receive({ type: 'challenge', use: mode, nonce: serverNonce, mac: overrideMac ?? hmac(proofKey, proof) });
  return { clientNonce, serverNonce };
}

function frame(key, state, seq, body, direction = 's2c') {
  const cipher = createCipheriv('aes-256-gcm', sessionKey(key, state), nonce(direction, seq));
  cipher.setAAD(aad(direction, state, seq));
  const encrypted = Buffer.concat([cipher.update(JSON.stringify(body), 'utf8'), cipher.final(), cipher.getAuthTag()]);
  return { type: 'secure', seq, ciphertext: encrypted.toString('hex') };
}

function sessionKey(key, state) {
  return Buffer.from(hmac(key, `sgt-browser-v8|aead|${state.clientNonce}|${state.serverNonce}`), 'hex');
}

function nonce(direction, seq) {
  const value = Buffer.alloc(12);
  value.writeUInt32BE(direction === 'c2s' ? 1 : 0, 0);
  value.writeBigUInt64BE(BigInt(seq), 4);
  return value;
}

function aad(direction, state, seq) {
  return Buffer.from(`sgt-browser-v8|frame|${direction}|${state.clientNonce}|${state.serverNonce}|${seq}`);
}

function decrypt(key, state, frame, direction = 'c2s') {
  const encrypted = Buffer.from(frame.ciphertext, 'hex');
  const decipher = createDecipheriv('aes-256-gcm', sessionKey(key, state), nonce(direction, frame.seq));
  decipher.setAAD(aad(direction, state, frame.seq));
  decipher.setAuthTag(encrypted.subarray(-16));
  return JSON.parse(Buffer.concat([decipher.update(encrypted.subarray(0, -16)), decipher.final()]).toString('utf8'));
}

module.exports = { harness, challenge, frame, decrypt, hmac, sleep, until, admissionSource };
