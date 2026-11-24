const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('playwright');

async function routeAssets(page) {
  const root = path.resolve(__dirname, '../dist');
  await page.route('**/*', async route => {
    const url = new URL(route.request().url());
    if (url.hostname !== 'promptdj.test') return route.abort();
    const relative = url.pathname === '/' ? 'index.html' : url.pathname.slice(1);
    const files = { 'index.html': 'text/html', 'assets/index.js': 'text/javascript', 'assets/index.css': 'text/css' };
    if (!files[relative]) return route.abort();
    await route.fulfill({ status: 200, contentType: files[relative], body: fs.readFileSync(path.join(root, relative)) });
  });
}

test('slider changes recording preview without sending native volume messages', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.addInitScript(() => { window.volumeMessages = []; window.ipc = { postMessage: message => window.volumeMessages.push(message) }; });
    await routeAssets(page);
    await page.goto('http://promptdj.test/');
    await page.waitForSelector('prompt-dj-midi');
    await page.evaluate(() => document.querySelector('prompt-dj-midi').showRecording(new Blob(['audio'])));
    const slider = page.locator('prompt-dj-midi').locator('.volume-slider');
    await slider.evaluate(input => { input.value = '0.25'; input.dispatchEvent(new Event('input', { bubbles: true })); });
    const preview = page.locator('prompt-dj-midi').locator('audio.audio-player');
    await assert.doesNotReject(() => preview.waitFor());
    assert.equal(await preview.evaluate(audio => audio.volume), 0.25);
    assert.deepEqual(await page.evaluate(() => window.volumeMessages.filter(message => message.startsWith('set_volume:'))), []);
  } finally {
    await browser.close();
  }
});

test('repeated settings retain the owner; key replacement retires it after preserving recording', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.addInitScript(() => {
      window.contexts = [];
      class Node {
        constructor() { this.disconnected = false; this.gain = { value: 1, setValueAtTime() {}, linearRampToValueAtTime() {}, setTargetAtTime(value) { this.value = value; } }; }
        connect() {}
        disconnect() { this.disconnected = true; }
      }
      window.AudioContext = class {
        constructor() { this.nodes = []; this.destination = {}; this.currentTime = 0; this.closed = false; window.contexts.push(this); }
        createGain() { const node = new Node(); this.nodes.push(node); return node; }
        createAnalyser() { return { frequencyBinCount: 8, getByteFrequencyData() {} }; }
        createMediaStreamDestination() { return { stream: {} }; }
        async decodeAudioData() { throw new Error('synthetic decoder failure'); }
        async suspend() {}
        async close() { this.closed = true; }
      };
      window.MediaRecorder = class {
        static isTypeSupported() { return true; }
        constructor() { this.state = 'inactive'; this.mimeType = 'audio/webm'; }
        start() { this.state = 'recording'; }
        stop() { this.state = 'inactive'; setTimeout(() => { this.ondataavailable({ data: new Blob([new Uint8Array(2048)]) }); this.onstop(); }, 0); }
      };
    });
    await routeAssets(page);
    await page.goto('http://promptdj.test/');
    await page.waitForSelector('prompt-dj-midi');
    const key = apiKey => page.evaluate(apiKey => window.postMessage({ type: 'pm-dj-set-api-key', apiKey }, '*'), apiKey);
    await key('first');
    await page.waitForFunction(() => window.contexts.length === 1);
    await key('first');
    await page.waitForTimeout(50);
    assert.equal(await page.evaluate(() => window.contexts.length), 1);
    await page.evaluate(() => window.postMessage({ type: 'pm-dj-start-recording' }, '*'));
    await page.waitForTimeout(50);
    await key('second');
    await page.waitForFunction(() => window.contexts.length === 2 && window.contexts[0].closed);
    assert.equal(await page.evaluate(() => window.contexts[0].nodes[0].disconnected), true);
    await page.locator('prompt-dj-midi').locator('audio.audio-player').waitFor();
  } finally {
    await browser.close();
  }
});
