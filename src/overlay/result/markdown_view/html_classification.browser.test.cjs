const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const repo = path.resolve(__dirname, '../../../..');
const { chromium } = require(path.join(repo, 'screen-record/node_modules/playwright'));
const fixture = JSON.parse(fs.readFileSync(
  path.join(repo, 'parity-fixtures/preset-system/result-overlay.json'), 'utf8',
));
const cases = new Map(fixture.markdown_view.raw_html_classification_cases.map((item) => [item.name, item]));

test('script double escaping keeps the apparent root close inside script text', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const name of ['script_double_escape_fake_close', 'script_double_escape_after_nonclosing_tag']) {
      const example = cases.get(name);
      assert.equal(example.raw, false, name);
      const scriptText = await page.evaluate((html) =>
        new DOMParser().parseFromString(html, 'text/html').querySelector('script')?.textContent,
      example.input);
      assert.ok(scriptText.includes('</html>'), name);
    }
  } finally {
    await browser.close();
  }
});

test('ordinary document comments remain outside the explicit root', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const example = cases.get('document_comments');
    assert.equal(example.raw, true);
    const result = await page.evaluate((html) => {
      const document = new DOMParser().parseFromString(html, 'text/html');
      return { doctype: document.doctype?.name, body: document.body.textContent, comments: [...document.childNodes].filter((node) => node.nodeType === Node.COMMENT_NODE).length };
    }, example.input);
    assert.equal(result.doctype, 'html');
    assert.equal(result.body, 'ok');
    assert.equal(result.comments, 3);
  } finally {
    await browser.close();
  }
});

test('unclosed SVG CDATA keeps apparent closing tags in foreign content', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const name of ['streaming_svg_script_cdata_fake_close', 'streaming_svg_cdata_fake_close']) {
      const example = cases.get(name);
      assert.equal(example.raw, false, name);
      const text = await page.evaluate((html) =>
        new DOMParser().parseFromString(html, 'text/html').querySelector('svg')?.textContent,
      example.input);
      assert.ok(text.includes('</html>'), name);
    }
  } finally {
    await browser.close();
  }
});

test('an SVG title integration point can leave a template open past an apparent root close', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const example = cases.get('svg_title_integration_template_open');
    assert.equal(example.raw, false);
    await page.setContent(example.input + '<script>window.afterRoot = true</script>');
    assert.equal(await page.evaluate(() => window.afterRoot === true), false);
    assert.equal(await page.locator('svg title template').count(), 1);
  } finally {
    await browser.close();
  }
});

test('CDATA-like markup at HTML integration points cannot hide an open template', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    for (const name of ['html_cdata_hides_template', 'svg_integration_cdata_hides_template', 'svg_title_cdata_hides_template']) {
      const page = await browser.newPage();
      const example = cases.get(name);
      assert.equal(example.raw, false, name);
      await page.setContent(example.input + '<script>window.afterRoot = true</script>');
      assert.equal(await page.evaluate(() => window.afterRoot === true), false, name);
      assert.equal(await page.locator('template').count(), 1, name);
      await page.close();
    }
  } finally {
    await browser.close();
  }
});

test('HTML-namespace CDATA and malformed attributes do not hide an earlier root close', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const cdata = cases.get('html_cdata_hides_root');
    const attribute = cases.get('unquoted_attribute_quote_hides_root');
    assert.equal(cdata.raw, false);
    assert.equal(attribute.raw, false);
    const cdataResult = await page.evaluate((html) => {
      const document = new DOMParser().parseFromString(html, 'text/html');
      return { comment: document.body.firstChild?.nodeValue, script: !!document.querySelector('script') };
    }, cdata.input);
    assert.equal(cdataResult.comment, '[CDATA[x');
    assert.equal(cdataResult.script, true);
    const attributeResult = await page.evaluate((html) => {
      const document = new DOMParser().parseFromString(html, 'text/html');
      return { value: document.querySelector('div')?.getAttribute('x'), script: !!document.querySelector('script') };
    }, attribute.input);
    assert.equal(attributeResult.value, 'a"');
    assert.equal(attributeResult.script, true);
    const equals = cases.get('unquoted_attribute_equals_quote_hides_root');
    assert.equal(equals.raw, false);
    const equalsResult = await page.evaluate((html) => {
      const document = new DOMParser().parseFromString(html, 'text/html');
      return { value: document.querySelector('div')?.getAttribute('x'), script: !!document.querySelector('script') };
    }, equals.input);
    assert.equal(equalsResult.value, 'a=');
    assert.equal(equalsResult.script, true);
  } finally {
    await browser.close();
  }
});

test('a well-formed public doctype still identifies the HTML document', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const example = cases.get('valid_public_doctype');
    assert.equal(example.raw, true);
    const result = await page.evaluate((html) => {
      const document = new DOMParser().parseFromString(html, 'text/html');
      return { publicId: document.doctype?.publicId, body: document.body.textContent };
    }, example.input);
    assert.equal(result.publicId, '-//Example//DTD HTML//EN');
    assert.equal(result.body, 'ok');
  } finally {
    await browser.close();
  }
});

test('root end tags inside open insertion contexts do not complete the document', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const samples = [
      ['streaming_template_fake_close', '<span id="late">late</span></template></body></html>', 'template'],
      ['streaming_select_fake_close', '<option id="late">late</option></select></body></html>', 'select'],
      ['streaming_table_fake_close', '<span id="late">late</span></td></tr></table></body></html>', 'table'],
      ['streaming_frameset_fake_close', '<frame id="late"></frameset></html>', 'frameset'],
    ];
    for (const [name, suffix, parent] of samples) {
      const example = cases.get(name);
      assert.equal(example.raw, false, name);
      const inside = await page.evaluate(({ html, parent }) => {
        const document = new DOMParser().parseFromString(html, 'text/html');
        const element = document.querySelector(parent);
        return parent === 'template' ? !!element?.content.querySelector('#late') : !!element?.querySelector('#late');
      }, { html: example.input + suffix, parent });
      assert.equal(inside, true, name);
    }
  } finally {
    await browser.close();
  }
});

test('a trailing slash in an unquoted foreign attribute is data, not a self-closing flag', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const [name, tag] of [['svg_unquoted_slash_is_value', 'svg'], ['math_unquoted_slash_is_value', 'math']]) {
      const example = cases.get(name);
      assert.equal(example.raw, false, name);
      const result = await page.evaluate(({ html, tag }) => {
        const document = new DOMParser().parseFromString(html + '<script id="late"></script>', 'text/html');
        const parent = document.querySelector(tag);
        return { value: parent?.getAttribute('x'), inside: !!parent?.querySelector('#late') };
      }, { html: example.input, tag });
      assert.equal(result.value, 'foo/', name);
      assert.equal(result.inside, true, name);
    }
  } finally {
    await browser.close();
  }
});

test('self-closing raw-like foreign tags depend on the actual namespace', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    for (const name of ['direct_svg_self_closing_title', 'direct_svg_self_closing_script', 'clean_nested_foreign_self_close']) {
      const page = await browser.newPage();
      const example = cases.get(name);
      assert.equal(example.raw, true, name);
      await page.setContent(example.input + '<script>window.afterRoot = true</script>');
      assert.equal(await page.evaluate(() => window.afterRoot === true), true, name);
      await page.close();
    }
    for (const name of ['foreign_breakout_self_closing_script', 'foreign_integration_self_closing_script', 'sensitive_exit_with_nested_foreign', 'foreign_breakout_in_nested_foreign']) {
      const page = await browser.newPage();
      const example = cases.get(name);
      assert.equal(example.raw, false, name);
      await page.setContent(example.input + '<script>window.afterRoot = true</script>');
      assert.equal(await page.evaluate(() => window.afterRoot === true), false, name);
      await page.close();
    }
  } finally {
    await browser.close();
  }
});
