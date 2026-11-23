import { registerHooks } from 'node:module';

// Source modules use bundler-style extensionless relative imports.
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier) && context.parentURL?.endsWith('.ts')) {
      return nextResolve(`${specifier}.ts`, context);
    }
    return nextResolve(specifier, context);
  },
});

class FakeParam {
  constructor() { this.value = 1; }
  setValueAtTime() {}
  linearRampToValueAtTime() {}
  setTargetAtTime(value) { this.value = value; }
}

class FakeNode {
  constructor() { this.gain = new FakeParam(); this.connections = []; this.disconnected = false; }
  connect(destination) { this.connections.push(destination); }
  disconnect() { this.disconnected = true; }
}

export class FakeAudioContext {
  constructor() {
    this.destination = new FakeNode();
    this.currentTime = 0;
    this.started = [];
    this.resumed = 0;
    this.closed = false;
  }
  createGain() { return new FakeNode(); }
  createBuffer(channels, length, sampleRate) {
    return { duration: length / sampleRate, channels: [], copyToChannel(data) { this.channels.push(Array.from(data)); } };
  }
  createBufferSource() {
    const context = this;
    const source = new FakeNode();
    source.start = when => context.started.push({ when, buffer: source.buffer });
    return source;
  }
  async resume() { this.resumed++; }
  async suspend() {}
  async close() { this.closed = true; }
}

globalThis.AudioContext = FakeAudioContext;

export function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

export function fakeSession() {
  const calls = [];
  const record = name => () => { calls.push(name); };
  return { calls, play: record('play'), pause: record('pause'), stop: record('stop'), setWeightedPrompts: async () => { calls.push('prompts'); } };
}

/** A GoogleGenAI stand-in whose live music connection resolves when the test says so. */
export function fakeAi() {
  const connections = [];
  return {
    connections,
    live: { music: { connect: ({ callbacks }) => { const pending = deferred(); connections.push({ callbacks, pending }); return pending.promise; } } },
  };
}

/** Base64 PCM: stereo int16 frames with the given left-channel sample values. */
export function pcmChunk(...leftSamples) {
  const bytes = new Uint8Array(leftSamples.length * 4);
  const view = new DataView(bytes.buffer);
  leftSamples.forEach((sample, index) => view.setInt16(index * 4, sample, true));
  return { data: Buffer.from(bytes).toString('base64'), mimeType: 'audio/l16' };
}

export const prompts = new Map([['p', { promptId: 'p', text: 'Funk', weight: 1, cc: 0, color: '#fff' }]]);
