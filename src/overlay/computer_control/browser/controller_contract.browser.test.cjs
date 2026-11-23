const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const { chromium } = require('../../../../screen-record/node_modules/playwright');

const controller = fs.readFileSync(path.join(__dirname, '../controller/browser.rs'), 'utf8');
const io = fs.readFileSync(path.join(__dirname, 'controller_io.rs'), 'utf8');
const fill = fs.readFileSync(path.join(__dirname, 'controller_io/fill.rs'), 'utf8');
const isolated = fs.readFileSync(path.join(__dirname, 'controller_isolated.rs'), 'utf8');
const perception = controller.match(/const PERCEPTION_JS: &str = r#"([\s\S]*?)"#;/)[1];
const documentId = isolated.match(/const DOCUMENT_ID_JS: &str = r#"([\s\S]*?)"#;/)[1];

function expression(name, expected) {
  const source = name === 'selection_expression' ? fill : io;
  const body = source.slice(source.indexOf(`fn ${name}(`));
  let js = body.match(/r#"([\s\S]*?)"#/)[1];
  for (const [key, value] of Object.entries({
    document_id: documentId,
    selector: JSON.stringify(expected.selector),
    expected_document: JSON.stringify(expected.documentId),
    expected_element: JSON.stringify(expected.elementId),
  })) js = js.replaceAll(`{${key}}`, value);
  return js.replaceAll('{{', '{').replaceAll('}}', '}');
}

function readBackExpression(expected) {
  let js = controller.slice(controller.indexOf('fn read_back(')).match(/r#"([\s\S]*?)"#/)[1];
  for (const [key, value] of Object.entries({
    document_identity: documentId,
    selector_json: JSON.stringify(expected.selector),
    document_json: JSON.stringify(expected.documentId),
    element_json: JSON.stringify(expected.elementId),
    expected_protected: JSON.stringify(expected.protected ?? false),
  })) js = js.replaceAll(`{${key}}`, value);
  return js.replaceAll('{{', '{').replaceAll('}}', '}');
}

test('page code cannot copy private controller identity to a replacement', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<form id="__sgtControllerIdentity"></form><input value="original">');
    await page.evaluate(() => {
      globalThis[Symbol.for('sgt.controller.private-state')] = { documentId: 'forged', nodes: new WeakMap() };
    });
    const cdp = await page.context().newCDPSession(page);
    const tree = await cdp.send('Page.getFrameTree');
    const frameId = tree.frameTree.frame.id;
    const world = await cdp.send('Page.createIsolatedWorld', { frameId, worldName: 'sgt-computer-control-identity-v1' });
    const again = await cdp.send('Page.createIsolatedWorld', { frameId, worldName: 'sgt-computer-control-identity-v1' });
    assert.equal(world.executionContextId, again.executionContextId);
    const evaluate = async expression => (await cdp.send('Runtime.evaluate', {
      expression, contextId: world.executionContextId, returnByValue: true, awaitPromise: true,
    })).result.value;
    const observation = await evaluate(perception);
    const expected = { selector: '[data-sgt-id="1"]', documentId: observation.documentId, elementId: observation.elements[0].elementId };
    assert.notEqual(observation.documentId, 'forged');
    assert.equal(await page.evaluate(() => globalThis[Symbol.for('sgt.controller.private-state')].documentId), 'forged');
    await page.evaluate(() => {
      const original = document.querySelector('input');
      const replacement = original.cloneNode(true);
      for (const key of Object.getOwnPropertySymbols(original)) replacement[key] = original[key];
      replacement.value = 'replacement';
      original.replaceWith(replacement);
    });
    const snapshot = await evaluate(expression('focus_target_expression', expected));
    assert.notEqual(snapshot.elementId, expected.elementId);
    assert.equal(snapshot.elementId, null);
    assert.equal(await page.evaluate(() => document.activeElement === document.querySelector('input')), false);
  } finally { await browser.close(); }
});

test('isolated child identity is recreated after an out-of-process frame navigates', async () => {
  const inner = http.createServer((_, response) => response.end('<button>Child</button>'));
  const outer = http.createServer((_, response) => response.end(`<iframe src="http://localhost:${inner.address().port}/first"></iframe>`));
  await new Promise(resolve => inner.listen(0, '0.0.0.0', resolve));
  await new Promise(resolve => outer.listen(0, '0.0.0.0', resolve));
  const browser = await chromium.launch({ headless: true, args: ['--site-per-process'] });
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${outer.address().port}/`);
    const frame = page.frames().find(item => item !== page.mainFrame());
    assert.ok(frame);
    const cdp = await page.context().newCDPSession(frame);
    async function world() {
      const tree = await cdp.send('Page.getFrameTree');
      return (await cdp.send('Page.createIsolatedWorld', {
        frameId: tree.frameTree.frame.id, worldName: 'sgt-computer-control-identity-v1',
      })).executionContextId;
    }
    async function evaluate(contextId, code) {
      return (await cdp.send('Runtime.evaluate', { expression: code, contextId, returnByValue: true })).result.value;
    }
    const first = await world();
    const old = await evaluate(first, perception);
    await frame.goto(`http://localhost:${inner.address().port}/second`);
    await assert.rejects(evaluate(first, '1'));
    const second = await world();
    const fresh = await evaluate(second, perception);
    assert.notEqual(fresh.documentId, old.documentId);
    const expected = { selector: '[data-sgt-id="1"]', documentId: old.documentId, elementId: old.elements[0].elementId };
    const snapshot = await evaluate(second, expression('selector_center_expression', expected));
    assert.notEqual(snapshot.documentId, expected.documentId);
  } finally {
    await browser.close();
    await Promise.all([new Promise(resolve => outer.close(resolve)), new Promise(resolve => inner.close(resolve))]);
  }
});

function selectExpression(kind, expected) {
  const body = controller.slice(controller.indexOf('    fn select('));
  const expressions = [...body.matchAll(/r#"([\s\S]*?)"#/g)];
  let js = expressions[kind === 'inspect' ? 0 : 1][1];
  for (const [key, value] of Object.entries({
    document_identity: documentId,
    selector_json: JSON.stringify(expected.selector),
    document_json: JSON.stringify(expected.documentId),
    element_json: JSON.stringify(expected.elementId),
    original_index: String(expected.index),
    expected_text: JSON.stringify(expected.text),
    expected_value: JSON.stringify(expected.value),
  })) js = js.replaceAll(`{${key}}`, value);
  return js.replaceAll('{{', '{').replaceAll('}}', '}');
}

test('select refuses disabled options and controls before dispatch', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<select><option value="a">A</option><option disabled value="b">B</option><optgroup disabled><option value="c">C</option></optgroup></select>');
    const observation = await page.evaluate(perception);
    const expected = { selector: '[data-sgt-id="1"]', documentId: observation.documentId, elementId: observation.elements[0].elementId, index: 1, text: 'B', value: 'b' };
    await page.evaluate(() => {
      window.selectEvents = [];
      const select = document.querySelector('select');
      for (const name of ['input', 'change']) select.addEventListener(name, () => window.selectEvents.push(name));
    });
    const inspection = await page.evaluate(selectExpression('inspect', expected));
    assert.deepEqual(inspection.options.map(option => option.disabled), [false, true, true]);
    for (const option of [expected, { ...expected, index: 2, text: 'C', value: 'c' }]) {
      assert.equal((await page.evaluate(selectExpression('apply', option))).reason, 'option_disabled');
    }
    await page.locator('select').evaluate(select => { select.disabled = true; });
    assert.equal((await page.evaluate(selectExpression('inspect', expected))).reason, 'select_disabled');
    assert.equal((await page.evaluate(selectExpression('apply', { ...expected, index: 0, text: 'A', value: 'a' }))).reason, 'select_disabled');
    assert.equal(await page.locator('select').evaluate(select => select.selectedIndex), 0);
    assert.deepEqual(await page.evaluate(() => window.selectEvents), []);
  } finally { await browser.close(); }
});

test('select rechecks optgroup disabled state after inspection', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<select><option value="a">A</option><optgroup><option value="b">B</option></optgroup></select>');
    const observation = await page.evaluate(perception);
    const expected = { selector: '[data-sgt-id="1"]', documentId: observation.documentId, elementId: observation.elements[0].elementId, index: 1, text: 'B', value: 'b' };
    assert.equal((await page.evaluate(selectExpression('inspect', expected))).options[1].disabled, false);
    await page.locator('optgroup').evaluate(group => { group.disabled = true; });
    assert.equal((await page.evaluate(selectExpression('apply', expected))).reason, 'option_disabled');
    assert.equal(await page.locator('select').evaluate(select => select.selectedIndex), 0);
  } finally { await browser.close(); }
});

test('select read-back reports selected text and value and detects deselection', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<select><option value="">Empty</option><option value="ca">Canada</option></select>');
    const observation = await page.evaluate(perception);
    const expected = { selector: '[data-sgt-id="1"]', documentId: observation.documentId, elementId: observation.elements[0].elementId };
    await page.locator('select').selectOption('ca');
    assert.deepEqual(await page.evaluate(readBackExpression(expected)), { protected: false, selectedText: 'Canada', value: 'ca', validity: null });
    await page.locator('select').evaluate(el => {
      el.addEventListener('change', () => { el.selectedIndex = -1; });
      el.selectedIndex = 0;
      el.dispatchEvent(new Event('change', { bubbles: true }));
    });
    assert.deepEqual(await page.evaluate(readBackExpression(expected)), { protected: false, selectedText: null, value: null, validity: null });
  } finally { await browser.close(); }
});

test('read-back distinguishes a protected value from a stale target', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<input type="password" value="secret">');
    const observation = await page.evaluate(perception);
    const expected = { selector: '[data-sgt-id="1"]', documentId: observation.documentId, elementId: observation.elements[0].elementId, protected: true };
    assert.deepEqual(await page.evaluate(readBackExpression(expected)), { protected: true, selectedText: null, value: null, validity: null });
    await page.locator('input').evaluate(el => { el.type = 'text'; });
    assert.equal(await page.evaluate(readBackExpression(expected)), null);
    await page.locator('input').evaluate(el => { el.type = 'password'; });
    await page.locator('input').evaluate(el => el.replaceWith(el.cloneNode()));
    assert.equal(await page.evaluate(readBackExpression(expected)), null);
  } finally { await browser.close(); }
});

test('a field that becomes protected after observation is not treated as verified dispatch', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<input type="text" value="content">');
    const observation = await page.evaluate(perception);
    const expected = { selector: '[data-sgt-id="1"]', documentId: observation.documentId, elementId: observation.elements[0].elementId };
    await page.locator('input').evaluate(el => { el.type = 'password'; });
    assert.equal(await page.evaluate(readBackExpression(expected)), null);
  } finally { await browser.close(); }
});

test('observations clear stale stamps and retain element identity', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<button id="first">First</button><button id="second">Second</button>');
    const first = await page.evaluate(perception);
    assert.equal(first.elements.length, 2);
    await page.locator('#first').evaluate(el => { el.style.display = 'none'; });
    const second = await page.evaluate(perception);
    assert.equal(second.documentId, first.documentId);
    assert.equal(second.elements.length, 1);
    assert.equal(second.elements[0].elementId, first.elements[1].elementId);
    assert.equal(await page.locator('[data-sgt-id="1"]').getAttribute('id'), 'second');
    assert.equal(await page.locator('#first').getAttribute('data-sgt-id'), null);
    const old = { selector: '[data-sgt-id="1"]', documentId: first.documentId, elementId: first.elements[0].elementId };
    const snapshot = await page.evaluate(expression('selector_center_expression', old));
    assert.equal(snapshot.elementId, second.elements[0].elementId);
    assert.notEqual(snapshot.elementId, old.elementId);
  } finally { await browser.close(); }
});

test('identity is checked before scroll, focus and selection', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<input id="target" value="hello">');
    const observation = await page.evaluate(perception);
    const expected = { selector: '[data-sgt-id="1"]', documentId: observation.documentId, elementId: observation.elements[0].elementId };
    await page.evaluate(() => {
      window.effects = [];
      const e = document.querySelector('input');
      e.scrollIntoView = () => window.effects.push('scroll');
      e.focus = () => window.effects.push('focus');
      e.select = () => window.effects.push('select');
    });
    for (const stale of [{ ...expected, elementId: 'old' }, { ...expected, documentId: 'old' }]) {
      await page.evaluate(expression('selector_center_expression', stale));
      await page.evaluate(expression('focus_target_expression', stale));
    }
    assert.deepEqual(await page.evaluate(() => window.effects), []);
    const fresh = await page.evaluate(expression('selector_center_expression', expected));
    assert.equal(fresh.interactable, true);
    assert.deepEqual(await page.evaluate(() => window.effects), ['scroll']);
  } finally { await browser.close(); }
});

test('focus handlers cannot cause selection of a replaced target', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<input value="hello">');
    const observation = await page.evaluate(perception);
    const expected = { selector: '[data-sgt-id="1"]', documentId: observation.documentId, elementId: observation.elements[0].elementId };
    await page.evaluate(() => {
      window.selected = false;
      const e = document.querySelector('input');
      e.select = () => { window.selected = true; };
      e.addEventListener('focus', () => e.replaceWith(e.cloneNode()));
    });
    const snapshot = await page.evaluate(expression('focus_target_expression', expected));
    assert.notEqual(snapshot.elementId, expected.elementId);
    assert.equal(await page.evaluate(() => window.selected), false);
  } finally { await browser.close(); }
});

test('fresh focus reports the identity, focus and geometry needed for insertion', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<input value="hello">');
    const observation = await page.evaluate(perception);
    const expected = { selector: '[data-sgt-id="1"]', documentId: observation.documentId, elementId: observation.elements[0].elementId };
    const snapshot = await page.evaluate(expression('focus_target_expression', expected));
    assert.equal(snapshot.documentId, expected.documentId);
    assert.equal(snapshot.elementId, expected.elementId);
    assert.equal(snapshot.focused, true);
    assert.equal(snapshot.interactable, true);
    assert.equal(await page.evaluate(() => document.querySelector('input').selectionEnd), 5);
  } finally { await browser.close(); }
});

test('fill selects the complete editable value before trusted insertion', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    for (const [html, selector, text, expectedValue] of [
      ['<input value="old">', 'input', 'new', 'new'],
      ['<textarea>old</textarea>', 'textarea', '', ''],
      ['<div contenteditable="true"><b>old</b><i> text</i></div>', 'div', 'new', 'new'],
      ['<div contenteditable="true">old</div>', 'div', '', ''],
      ['<div contenteditable="true"></div>', 'div', 'new', 'new'],
    ]) {
      const page = await browser.newPage();
      await page.setContent(html);
      const observed = await page.evaluate(perception);
      const expected = { selector: '[data-sgt-id="1"]', documentId: observed.documentId, elementId: observed.elements[0].elementId };
      const cdp = await page.context().newCDPSession(page);
      const focused = await page.evaluate(expression('focus_target_expression', expected));
      assert.equal(focused.selectionReady, true);
      assert.equal((await page.evaluate(expression('selection_expression', expected))).selectionReady, true);
      await cdp.send('Input.insertText', { text });
      const value = await page.locator(selector).evaluate(e => 'value' in e ? e.value : e.innerText);
      assert.equal(value, expectedValue);
      await page.close();
    }
  } finally { await browser.close(); }
});

test('fill selection guard rejects changed selection or replaced editable', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<div contenteditable="true"><b>old</b> text</div>');
    const observed = await page.evaluate(perception);
    const expected = { selector: '[data-sgt-id="1"]', documentId: observed.documentId, elementId: observed.elements[0].elementId };
    assert.equal((await page.evaluate(expression('focus_target_expression', expected))).selectionReady, true);
    await page.evaluate(() => getSelection().collapse(document.querySelector('b').firstChild, 1));
    assert.equal((await page.evaluate(expression('selection_expression', expected))).selectionReady, false);
    await page.evaluate(() => { const e = document.querySelector('div'); e.replaceWith(e.cloneNode(true)); });
    assert.equal((await page.evaluate(expression('selection_expression', expected))).selectionReady, false);
  } finally { await browser.close(); }
});

test('fill reports input types without a verifiable selection as unsupported', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    for (const type of ['email', 'number']) {
      const page = await browser.newPage();
      await page.setContent(`<input type="${type}" value="123">`);
      const observed = await page.evaluate(perception);
      const expected = { selector: '[data-sgt-id="1"]', documentId: observed.documentId, elementId: observed.elements[0].elementId };
      const result = await page.evaluate(expression('focus_target_expression', expected));
      assert.equal(result.selectionUnsupported, true);
      assert.equal(result.selectionReady, false);
      assert.equal(await page.locator('input').inputValue(), '123');
      await page.close();
    }
  } finally { await browser.close(); }
});

test('fill selects and replaces an editable in a child frame', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<iframe srcdoc="<div contenteditable=true><b>old</b> text</div>"></iframe>');
    const frame = page.frames().find(item => item !== page.mainFrame());
    const observed = await frame.evaluate(perception);
    const expected = { selector: '[data-sgt-id="1"]', documentId: observed.documentId, elementId: observed.elements[0].elementId };
    assert.equal((await frame.evaluate(expression('focus_target_expression', expected))).selectionReady, true);
    assert.equal((await frame.evaluate(expression('selection_expression', expected))).selectionReady, true);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Input.insertText', { text: 'new' });
    assert.equal(await frame.locator('div').innerText(), 'new');
  } finally { await browser.close(); }
});

test('scroll settlement reports a replacement instead of the original element', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    await page.setContent('<button>Press</button>');
    const observation = await page.evaluate(perception);
    const expected = { selector: '[data-sgt-id="1"]', documentId: observation.documentId, elementId: observation.elements[0].elementId };
    await page.evaluate(() => {
      const e = document.querySelector('button');
      e.scrollIntoView = () => requestAnimationFrame(() => e.replaceWith(e.cloneNode()));
    });
    const snapshot = await page.evaluate(expression('selector_center_expression', expected));
    assert.notEqual(snapshot.elementId, expected.elementId);
  } finally { await browser.close(); }
});
