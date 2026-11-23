const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const frontendRequire = createRequire(require.resolve('../../../../screen-record/package.json'));
const { JSDOM } = frontendRequire('jsdom');

function surface() {
    const dom = new JSDOM('<div id="body"></div>', {
        url: 'http://127.0.0.1:1234/index.html', runScripts: 'dangerously'
    });
    dom.window.eval(fs.readFileSync(path.join(__dirname, 'dom_patch_runtime.js'), 'utf8'));
    return { window: dom.window, body: dom.window.document.getElementById('body') };
}

test('shared card strips active markup while retaining formatted tables and links', () => {
    const { window, body } = surface();
    window.__SGT_PATCH_BODY__(body, `<p class="word" onclick="window.compromised=1">Hi
      <img src="x" onerror="window.compromised=2"><a href="javascript:window.compromised=3">link</a>
      <svg onload="window.compromised=4"><script>window.compromised=5</script></svg>
      <form action="https://example.test"><button>submit</button></form></p>
      <table><thead><tr><th style="text-align: right">Number</th></tr></thead>
      <tbody><tr><td>42</td></tr></tbody></table><a href="https://example.test/">safe</a>`);
    body.querySelector('img').dispatchEvent(new window.Event('error'));
    assert.equal(window.compromised, undefined);
    assert.equal(body.querySelector('[onclick], [onerror], svg, script, form'), null);
    assert.equal(body.querySelector('a').hasAttribute('href'), false);
    assert.equal(body.querySelector('th').style.textAlign, 'right');
    assert.equal(body.querySelectorAll('a')[1].href, 'https://example.test/');
    assert.equal(body.querySelector('td').textContent, '42');
});

test('streamed changes and source replacement use the same sanitized fragment', () => {
    const { window, body } = surface();
    window.__SGT_PATCH_BODY__(body, '<p><img src="https://example.test/a.png"></p>');
    window.__SGT_PATCH_BODY__(body, '<p><img src="javascript:alert(1)" onerror="alert(1)"></p>');
    assert.equal(body.querySelector('img').hasAttribute('src'), false);
    assert.equal(body.querySelector('img').hasAttribute('onerror'), false);
    body.replaceChildren(window.__SGT_SAFE_FRAGMENT__('<p>hello <b onclick="alert(1)">world</b><i>i</i><u>u</u><sub>1</sub><sup>2</sup></p>'));
    assert.equal(body.textContent, 'hello worldiu12');
    for (const tag of ['b', 'i', 'u', 'sub', 'sup']) assert.ok(body.querySelector(tag));
    assert.equal(body.querySelector('[onclick]'), null);
});

test('bundled grid renders sanitized table markup without restoring handlers', async () => {
    const { window, body } = surface();
    window.eval(fs.readFileSync(path.join(__dirname, 'vendor/gridjs-6.2.0.min.js'), 'utf8'));
    window.__SGT_PATCH_BODY__(body, '<table><thead><tr><th>Name</th></tr></thead><tbody><tr><td><img src="x" onerror="window.compromised=1">item</td></tr></tbody></table>');
    const wrapper = window.document.createElement('div');
    body.appendChild(wrapper);
    new window.gridjs.Grid({ from: body.querySelector('table'), sort: true }).render(wrapper);
    await new Promise(resolve => setTimeout(resolve, 50));
    assert.ok(wrapper.querySelector('.gridjs-table'));
    for (const image of wrapper.querySelectorAll('img')) image.dispatchEvent(new window.Event('error'));
    assert.equal(window.compromised, undefined);
    assert.equal(wrapper.querySelector('[onerror]'), null);
});

test('streaming keeps unchanged nodes and updates changed text in place', () => {
    const { body, window } = surface();
    window.__SGT_PATCH_BODY__(body, '<p>first <b>bold</b></p><p>second</p>');
    const [first, second] = body.querySelectorAll('p');
    const bold = body.querySelector('b');
    const secondText = second.firstChild;
    window.__SGT_PATCH_BODY__(body, '<p>first <b>bold</b></p><p>second, longer</p><p>third</p>');
    const paragraphs = body.querySelectorAll('p');
    assert.equal(paragraphs.length, 3);
    assert.equal(paragraphs[0], first, 'an unchanged paragraph keeps its element');
    assert.equal(body.querySelector('b'), bold);
    assert.equal(paragraphs[1], second);
    assert.equal(paragraphs[1].firstChild, secondText, 'changed text reuses its text node');
    assert.equal(secondText.nodeValue, 'second, longer');
    assert.equal(paragraphs[2].textContent, 'third');
});
