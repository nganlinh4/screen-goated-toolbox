const test = require('node:test');
const assert = require('node:assert/strict');
const http = require('node:http');
const { chromium } = require('../../../../screen-record/node_modules/playwright');

test('CDP frame URL separates fragments across same-document history', async () => {
  const server = http.createServer((_, response) => response.end('<p>route</p>'));
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const cdp = await page.context().newCDPSession(page);
    const base = `http://127.0.0.1:${server.address().port}/`;
    await page.goto(`${base}#one`);
    const first = (await cdp.send('Page.getFrameTree')).frameTree.frame;
    assert.equal(first.url, base);
    assert.equal(first.urlFragment, '#one');
    await page.evaluate(() => { location.hash = '#two'; });
    const second = (await cdp.send('Page.getFrameTree')).frameTree.frame;
    assert.equal(second.urlFragment, '#two');
    assert.equal(second.loaderId, first.loaderId);
    await page.goBack();
    assert.equal((await cdp.send('Page.getFrameTree')).frameTree.frame.urlFragment, '#one');
    await page.evaluate(() => { location.hash = '#'; });
    assert.equal((await cdp.send('Page.getFrameTree')).frameTree.frame.urlFragment, '#');
  } finally {
    await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
});
