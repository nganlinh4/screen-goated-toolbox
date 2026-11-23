const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const { chromium } = require('../../../screen-record/node_modules/playwright');

const script = fs.readFileSync(path.join(__dirname, 'navigation_runtime.js'), 'utf8');

test('synthetic page events cannot request privileged host effects; a real link stays in the view', async () => {
  const server = http.createServer((req, res) => {
    res.setHeader('content-type', 'text/html');
    res.end(req.url === '/destination' ? '<p>destination</p>' :
      '<a id="link" href="/destination" target="_blank">open</a>');
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    let popups = 0;
    page.on('popup', () => popups++);
    await page.addInitScript(() => {
      window.sentMessages = [];
      window.ipc = { postMessage: message => window.sentMessages.push(message) };
    });
    await page.addInitScript({ content: script });
    const base = `http://127.0.0.1:${server.address().port}`;
    await page.goto(base);
    const synthetic = await page.evaluate(() => {
      const link = document.querySelector('#link');
      link.addEventListener('click', event => { if (!event.isTrusted) event.preventDefault(); });
      document.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
      link.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      return { target: link.target, sent: window.sentMessages };
    });
    assert.deepEqual(synthetic, { target: '_blank', sent: [] });
    await Promise.all([page.waitForURL(`${base}/destination`), page.locator('#link').click()]);
    assert.equal(popups, 0);
  } finally {
    await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
});
