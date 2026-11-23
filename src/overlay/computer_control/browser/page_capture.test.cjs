const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, 'page_capture.js'), 'utf8');
const SHOW_ELEMENT = 1;
const SHOW_TEXT = 4;

// A minimal fixed DOM: enough surface for the capture walk, with a counter on
// every computed-style read.
function fakeDocument() {
  const stats = { styleReads: 0, elements: 0 };
  const doc = {
    title: 'fixture', baseURI: 'https://fixture.test/',
    defaultView: { getComputedStyle(el) { stats.styleReads++; return el.style; } },
    getElementsByTagName: () => [],
    querySelectorAll: () => [],
    createTreeWalker(root, mask) {
      const nodes = [];
      const visit = (node) => {
        for (const child of node.childNodes) {
          if ((child.nodeType === 1 && mask & SHOW_ELEMENT) || (child.nodeType === 3 && mask & SHOW_TEXT)) nodes.push(child);
          if (child.nodeType === 1) visit(child);
        }
      };
      visit(root);
      let index = 0;
      return { nextNode: () => nodes[index++] || null };
    },
  };
  const element = (tag, parent, { style = {}, attrs = {}, hidden = false } = {}) => {
    stats.elements++;
    const el = {
      nodeType: 1, tagName: tag.toUpperCase(), parentElement: parent, ownerDocument: doc, hidden,
      childNodes: [], children: [],
      style: { display: 'block', visibility: 'visible', opacity: '1', ...style },
      getAttribute: name => attrs[name] ?? null,
      hasAttribute: name => name in attrs,
      getClientRects: () => [{}],
      getRootNode: () => doc,
      matches: () => false, closest: () => null,
      querySelectorAll: () => [], querySelector: () => null,
    };
    if (parent) { parent.childNodes.push(el); parent.children.push(el); }
    return el;
  };
  const text = (parent, value) => {
    parent.childNodes.push({ nodeType: 3, nodeValue: value, parentElement: parent });
  };
  doc.documentElement = element('html', null);
  doc.body = element('body', doc.documentElement);
  return { doc, stats, element, text };
}

function capture(doc) {
  return vm.runInNewContext(source, {
    document: doc, location: { href: 'https://fixture.test/' },
    NodeFilter: { SHOW_ELEMENT, SHOW_TEXT }, Node: { ELEMENT_NODE: 1, TEXT_NODE: 3 }, URL,
  });
}

test('deep documents read each element style once per capture', () => {
  const { doc, stats, element, text } = fakeDocument();
  let parent = doc.body;
  for (let depth = 0; depth < 40; depth++) parent = element('div', parent);
  for (let index = 0; index < 400; index++) text(element('span', parent), `cell ${index}`);
  const result = capture(doc);
  assert.equal(result.eligibleTextNodes, 400);
  assert.ok(
    stats.styleReads <= stats.elements,
    `${stats.styleReads} computed-style reads for ${stats.elements} elements`,
  );
});

test('memoized visibility still hides every descendant of a hidden ancestor', () => {
  const { doc, element, text } = fakeDocument();
  const hiddenStyle = element('section', doc.body, { style: { display: 'none' } });
  const ariaHidden = element('section', doc.body, { attrs: { 'aria-hidden': 'true' } });
  const transparent = element('section', doc.body, { style: { opacity: '0' } });
  const shown = element('section', doc.body);
  for (const [root, label] of [[hiddenStyle, 'display'], [ariaHidden, 'aria'], [transparent, 'opacity'], [shown, 'shown']]) {
    const inner = element('div', element('div', root));
    text(inner, `${label} first`);
    text(element('p', inner), `${label} nested`);
    text(root, `${label} direct`);
  }
  const lines = capture(doc).text.split('\n');
  assert.deepEqual(lines, ['shown first', 'shown nested', 'shown direct']);
});
