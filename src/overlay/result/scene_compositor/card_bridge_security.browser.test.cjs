const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');

const root = path.resolve(__dirname, '../../../..');
const { chromium } = createRequire(path.join(root, 'screen-record/package.json'))('@playwright/test');
const bridgeSource = fs.readFileSync(path.join(__dirname, 'card_bridge.rs'), 'utf8');
const bridge = bridgeSource.match(/const BRIDGE: &str = r#"([\s\S]*?)"#;/)[1];
const scene = fs.readFileSync(path.join(__dirname, 'scene_runtime.js'), 'utf8') +
  fs.readFileSync(path.join(__dirname, 'scene_frame_runtime.js'), 'utf8');
const diagnosticPolicy = scene.slice(scene.indexOf('let frameDiagnosticStart'), scene.indexOf('function reportCardDiagnostic'));
const frameHandler = scene.slice(scene.indexOf("window.addEventListener('message', function(event) {"));

test('untrusted page cannot forge effects or steal the private port', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const pageErrors = [];
    page.on('pageerror', error => pageErrors.push(error.message));
    await page.setContent('<!doctype html><body></body>');
    const authored = String.raw`<html><body>
      <p id="text">selected text</p><a id="link" href="https://example.com/expected">open link</a>
      <script>
      window.observed = { portTargets: 0, sentPorts: 0, windowPorts: 0 };
      const data = Object.getOwnPropertyDescriptor(MessageEvent.prototype, 'data').get;
      Object.defineProperty(MessageEvent.prototype, 'data', { configurable: true, get() {
        if (this.target instanceof MessagePort) observed.portTargets++;
        return data.call(this);
      }});
      const post = MessagePort.prototype.postMessage;
      MessagePort.prototype.postMessage = function(...args) { observed.sentPorts++; return post.apply(this, args); };
      window.addEventListener('message', e => { observed.windowPorts += e.ports.length; });
      window.getSelection = () => ({ toString: () => 'forged selection' });
      Selection.prototype.toString = () => 'forged selection';
      Element.prototype.closest = () => null;
      RegExp.prototype.exec = () => null;
      String.prototype.startsWith = () => false;
      Object.defineProperty(HTMLAnchorElement.prototype, 'href', { configurable: true, get() {
        return 'https://example.invalid/forged';
      }});
      for (let i = 0; i < 1000; i++) parent.postMessage({
        type: 'card_interaction', card_id: '7', document_revision: 1
      }, '*');
      for (let i = 0; i < 1000; i++) parent.postMessage({
        type: 'card_diagnostic', phase: 'interactive_document_alive', card_id: '7', document_revision: 1
      }, '*');
      parent.postMessage({ type: 'copy_selection', text: 'forged', card_id: '7', document_revision: 1 }, '*');
      parent.postMessage({ type: 'card_navigation', url: 'https://example.invalid/', card_id: '7', document_revision: 1 }, '*');
      parent.postMessage({ type: 'card_diagnostic', phase: 'bridge_ready', card_id: { toString: 5 }, document_revision: 1 }, '*');
      const fake = new MessageChannel();
      parent.postMessage({ type: 'card_diagnostic', phase: 'bridge_ready', card_id: '7', document_revision: 1 }, '*', [fake.port2]);
      fake.port1.postMessage({ type: 'copy_selection', text: 'forged through fake port' });
      document.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      document.getElementById('text').dispatchEvent(new MouseEvent('click', { bubbles: true }));
      window.__SGT_RUN_FIT__ = () => {
        const cyclic = {}; cyclic.self = cyclic;
        window.__SGT_FIT_CONTEXT__.reportDiagnostic(cyclic);
        window.__SGT_FIT_CONTEXT__.reportDiagnostic({ large: 'x'.repeat(10000) });
        for (let i = 0; i < 1000; i++) window.__SGT_FIT_CONTEXT__.reportDiagnostic({ i });
      };
      </script>
    </body></html>`;
    await page.evaluate(({ frameHandler, diagnosticPolicy, bridge, authored }) => {
      const events = [];
      window.__events = events;
      window.ipc = { postMessage: value => events.push(JSON.parse(value)) };
      const cards = new Map();
      const frame = document.createElement('iframe');
      frame.setAttribute('sandbox', 'allow-scripts');
      frame.style.cssText = 'width:500px;height:200px';
      const entry = { frame, mode: 'isolated', revision: 1, commandPort: null,
        card: { dataset: { id: '7' } }, visible: true, contentRevision: 1,
        activateIsolatedBridge() { window.__ready = true; } };
      cards.set('7', entry);
      window.__entry = entry;
      function queueFit() {}
      function completeFit() {}
      function raiseCard() {}
      function navigateTo(entry, url, epoch) { window.ipc.postMessage(JSON.stringify({
        type: 'navigation_request', id: Number(entry.card.dataset.id), url: String(url), topology_epoch: epoch
      })); }
      function postCardMessage() {}
      function reportCardDiagnostic(id, entry, phase, details) { events.push({ phase, ...details }); }
      function deferIsolatedSettledPaint() { return false; }
      function isolatedSurfaceVisibility() { return { visible: true }; }
      eval(diagnosticPolicy + frameHandler);
      frame.srcdoc = '<!--sgt-card:7:1--><!doctype html>' + bridge + authored;
      document.body.appendChild(frame);
    }, { frameHandler, diagnosticPolicy, bridge, authored });
    await page.waitForFunction(() => window.__ready === true);
    await page.waitForTimeout(100);
    let effects = await page.evaluate(() => window.__events.filter(e =>
      e.action === 'copy_selection' || e.type === 'navigation_request' || e.type === 'interaction'));
    assert.deepEqual(effects, []);

    await page.evaluate(() => window.__entry.commandPort.postMessage({ type: 'run_fit', card_id: '7' }));
    await page.waitForTimeout(100);
    const child = page.frameLocator('iframe');
    await child.locator('#link').click();
    await page.waitForTimeout(100);
    effects = await page.evaluate(() => window.__events.filter(e =>
      e.action === 'copy_selection' || e.type === 'navigation_request' || e.type === 'interaction'));
    assert.equal(effects.filter(e => e.type === 'navigation_request').length, 1);
    assert.equal(effects.find(e => e.type === 'navigation_request').url, 'https://example.com/expected');
    assert.equal(effects.filter(e => e.type === 'interaction').length, 1);
    assert.equal(effects.find(e => e.type === 'navigation_request').topology_epoch, '1');
    await page.evaluate(() => {
      window.__SGT_TOPOLOGY_EPOCH__ = '2';
      window.__entry.commandPort.postMessage({ type: 'topology_epoch', card_id: '7', epoch: '2' });
    });
    await page.waitForTimeout(100);

    await child.locator('#text').click();
    const frame = page.frames().find(item => item !== page.mainFrame());
    await frame.evaluate(() => {
      const range = document.createRange();
      range.selectNodeContents(document.getElementById('text'));
      const selection = document.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
    });
    await page.keyboard.press('Control+C');
    await page.waitForTimeout(100);
    const result = await page.evaluate(() => ({
      copies: window.__events.filter(e => e.action === 'copy_selection'),
      diagnostics: window.__events.filter(e => e.type === 'fit_diagnostic').length,
      interactiveReports: window.__events.filter(e => e.phase === 'interactive_document_alive').length,
    }));
    assert.deepEqual(result.copies.map(e => e.text), ['selected text']);
    assert.equal(result.copies[0].topology_epoch, '2');
    assert.ok(result.diagnostics <= 16);
    assert.equal(result.interactiveReports, 1);
    assert.deepEqual(pageErrors, []);
    // The one intercepted send is the attacker's own fake port, not the bridge.
    assert.deepEqual(await frame.evaluate(() => window.observed), { portTargets: 0, sentPorts: 1, windowPorts: 0 });
  } finally {
    await browser.close();
  }
});
