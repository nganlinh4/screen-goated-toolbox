const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const { chromium } = require('../../../../screen-record/node_modules/playwright');

const isolated = fs.readFileSync(path.join(__dirname, 'controller_isolated.rs'), 'utf8');
const privateId = isolated.match(/const DOCUMENT_ID_JS: &str = r#"([\s\S]*?)"#;/)[1];
const binding = fs.readFileSync(path.join(__dirname, 'document_binding.rs'), 'utf8');
const mainWorldCall = JSON.parse(binding.match(/const MAIN_WORLD_CALL: &str\s*=\s*("(?:[^"\\]|\\.)*");/)[1]);
const worldName = 'sgt-computer-control-identity-v1';

// Bind the same browser-owned DOM root in both worlds. The document object in
// the default context stays tied to that document, even if a navigation wins
// between the private check and the actual main-world call.
async function bindDocument(cdp) {
  const tree = await cdp.send('Page.getFrameTree');
  const context = (await cdp.send('Page.createIsolatedWorld', {
    frameId: tree.frameTree.frame.id, worldName,
  })).executionContextId;
  const root = (await cdp.send('DOM.getDocument', { depth: 0 })).root;
  const isolatedObject = (await cdp.send('DOM.resolveNode', {
    backendNodeId: root.backendNodeId, executionContextId: context,
  })).object.objectId;
  const identity = await cdp.send('Runtime.callFunctionOn', {
    objectId: isolatedObject,
    functionDeclaration: `function() { return this === document ? (${privateId}) : null; }`,
    returnByValue: true,
  });
  assert.ok(identity.result.value);
  const mainObject = (await cdp.send('DOM.resolveNode', { backendNodeId: root.backendNodeId })).object.objectId;
  return { documentId: identity.result.value, root, mainObject };
}

async function execute(cdp, objectId, code) {
  return cdp.send('Runtime.callFunctionOn', {
    objectId, functionDeclaration: mainWorldCall,
    arguments: [{ value: code }], returnByValue: true, awaitPromise: true,
  });
}

test('page-visible nonce spoof cannot change the isolated document identity', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.goto('data:text/html,<title>old</title>');
    const cdp = await page.context().newCDPSession(page);
    const before = await bindDocument(cdp);
    await page.evaluate(() => {
      globalThis[Symbol.for('sgt.controller.document-id')] = 'forged';
      globalThis[Symbol.for('sgt.controller.private-state')] = { documentId: 'forged', nodes: new WeakMap() };
    });
    assert.equal((await bindDocument(cdp)).documentId, before.documentId);
    await page.goto('data:text/html,<title>new</title>');
    await page.evaluate(id => {
      globalThis[Symbol.for('sgt.controller.document-id')] = id;
      globalThis[Symbol.for('sgt.controller.private-state')] = { documentId: id, nodes: new WeakMap() };
    }, before.documentId);
    assert.notEqual((await bindDocument(cdp)).documentId, before.documentId);
  } finally { await browser.close(); }
});

test('document-bound arbitrary JavaScript stays in the main world and old objects cannot reach a new document', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.goto('data:text/html,<title>old</title>');
    const cdp = await page.context().newCDPSession(page);
    const before = await bindDocument(cdp);
    const value = await execute(cdp, before.mainObject,
      'globalThis.pageOwned = 42; Promise.resolve({title: document.title, main: window === globalThis})');
    assert.deepEqual(value.result.value, { guard: 'ok', result: { title: 'old', main: true } });
    assert.equal(await page.evaluate(() => globalThis.pageOwned), 42);
    const undefinedResult = await execute(cdp, before.mainObject, 'globalThis.changedBeforeUndefined = true; void 0');
    assert.deepEqual(undefinedResult.result.value, { guard: 'ok' });
    assert.equal(await page.evaluate(() => globalThis.changedBeforeUndefined), true);
    await page.goto('data:text/html,<title>new</title>');
    await assert.rejects(execute(cdp, before.mainObject, 'globalThis.mutated = true; 1'));
    assert.equal(await page.evaluate(() => globalThis.mutated), undefined);
    await assert.rejects(cdp.send('DOM.resolveNode', { backendNodeId: before.root.backendNodeId }));
  } finally { await browser.close(); }
});

test('same-document hash changes retain the bound main-world document', async () => {
  const server = http.createServer((_, response) => response.end('<title>stable</title>'));
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    const cdp = await page.context().newCDPSession(page);
    const before = await bindDocument(cdp);
    await page.evaluate(() => { location.hash = 'section'; });
    const after = await bindDocument(cdp);
    assert.equal(after.documentId, before.documentId);
    assert.equal(after.root.backendNodeId, before.root.backendNodeId);
    const result = await execute(cdp, before.mainObject, 'location.hash');
    assert.deepEqual(result.result.value, { guard: 'ok', result: '#section' });
  } finally {
    await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
});

test('an old upload backend node cannot target its same-selector replacement', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<input id="upload" type="file">');
    const cdp = await page.context().newCDPSession(page);
    const before = await bindDocument(cdp);
    const query = await cdp.send('DOM.querySelector', { nodeId: before.root.nodeId, selector: '#upload' });
    const oldNode = (await cdp.send('DOM.describeNode', { nodeId: query.nodeId })).node.backendNodeId;
    await page.locator('#upload').evaluate(el => {
      globalThis.detachedUpload = el;
      globalThis.uploadEvents = [];
      el.addEventListener('input', () => globalThis.uploadEvents.push('input'));
      el.addEventListener('change', () => globalThis.uploadEvents.push('change'));
      el.replaceWith(el.cloneNode());
    });
    const freshRoot = (await cdp.send('DOM.getDocument', { depth: 0 })).root;
    const fresh = await cdp.send('DOM.querySelector', { nodeId: freshRoot.nodeId, selector: '#upload' });
    const newNode = (await cdp.send('DOM.describeNode', { nodeId: fresh.nodeId })).node.backendNodeId;
    assert.notEqual(oldNode, newNode);
    let dispatched = false;
    try {
      await cdp.send('DOM.setFileInputFiles', { backendNodeId: oldNode, files: [path.resolve(__dirname, '../../../../screen-record/package.json')] });
      dispatched = true;
    } catch (error) {
      // Chromium may reject the disconnected node, which is also safe.
      assert.match(error.message, /node|document|element/i);
    }
    assert.equal(await page.locator('#upload').evaluate(el => el.files.length), 0);
    if (dispatched) {
      assert.deepEqual(await page.evaluate(() => ({
        files: globalThis.detachedUpload.files.length, events: globalThis.uploadEvents,
      })), { files: 1, events: ['input', 'change'] });
    }
    const postRoot = (await cdp.send('DOM.getDocument', { depth: 0 })).root;
    const postQuery = await cdp.send('DOM.querySelector', { nodeId: postRoot.nodeId, selector: '#upload' });
    const postNode = (await cdp.send('DOM.describeNode', { nodeId: postQuery.nodeId })).node.backendNodeId;
    assert.notEqual(postNode, oldNode);
    await cdp.send('DOM.setFileInputFiles', {
      backendNodeId: newNode, files: [path.resolve(__dirname, '../../../../screen-record/package.json')],
    });
    assert.equal(await page.locator('#upload').evaluate(el => el.files[0].name), 'package.json');
  } finally { await browser.close(); }
});

test('a cross-origin child document object dies with its own frame context', async () => {
  const child = http.createServer((req, response) => response.end(`<title>${req.url}</title>`));
  const parent = http.createServer((_, response) => response.end(`<iframe src="http://localhost:${child.address().port}/old"></iframe>`));
  await new Promise(resolve => child.listen(0, '0.0.0.0', resolve));
  await new Promise(resolve => parent.listen(0, '0.0.0.0', resolve));
  const browser = await chromium.launch({ headless: true, args: ['--site-per-process'] });
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${parent.address().port}/`);
    const frame = page.frames().find(candidate => candidate !== page.mainFrame());
    assert.ok(frame);
    const cdp = await page.context().newCDPSession(frame);
    const before = await bindDocument(cdp);
    assert.deepEqual((await execute(cdp, before.mainObject, 'document.title')).result.value,
      { guard: 'ok', result: '/old' });
    await frame.goto(`http://localhost:${child.address().port}/new`);
    await assert.rejects(execute(cdp, before.mainObject, 'globalThis.changed = true; 1'));
    assert.equal(await frame.evaluate(() => globalThis.changed), undefined);
    assert.notEqual((await bindDocument(cdp)).documentId, before.documentId);
  } finally {
    await browser.close();
    await Promise.all([new Promise(resolve => parent.close(resolve)), new Promise(resolve => child.close(resolve))]);
  }
});
