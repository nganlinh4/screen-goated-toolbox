const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require('../screen-record/node_modules/playwright');

const repo = path.resolve(__dirname, '..');
const source = fs.readFileSync(path.join(repo, 'src/overlay/recording/ui.rs'), 'utf8');
const icons = fs.readFileSync(path.join(repo, 'src/overlay/html_components/icons.rs'), 'utf8');
const locale = fs.readFileSync(path.join(repo, 'src/gui/locale/vi/shell.rs'), 'utf8');
const paused = locale.match(/recording_paused: "([^"]+)"/)[1];
const hint = locale.match(/recording_subtext: "([^"]+)"/)[1];
const font = fs.readFileSync(path.join(repo, 'assets/GoogleSansFlex-VariableFont_GRAD,ROND,opsz,slnt,wdth,wght.ttf')).toString('base64');

function documentHtml(dark, width) {
  const icon = name => icons.match(new RegExp('"' + name + '" => \\{\\s*r#"([\\s\\S]*?)"#'))[1];
  const values = {
    font_css: `@font-face {font-family:'Google Sans Flex';font-weight:1 1000;font-stretch:25% 151%;src:url(data:font/ttf;base64,${font}) format('truetype');}`,
    width: width - 20, height: 50, is_dark: dark,
    container_bg: dark ? '#121212' : '#fafafa', container_border: dark ? '#333' : '#ddd',
    text_color: dark ? 'white' : '#222', subtext_color: dark ? '#bbb' : '#666',
    btn_bg: dark ? '#222' : '#eee', btn_hover_bg: dark ? '#333' : '#ddd',
    btn_color: dark ? '#ccc' : '#333', text_shadow: 'none',
    tx_rec: 'Recording', tx_proc: 'Processing', tx_wait: 'Starting', tx_init: 'Connecting',
    tx_paused: paused, tx_sub: hint.replace('{hotkey}', 'Esc'),
    icon_play: icon('play_arrow'), icon_pause: icon('pause'),
    icon_close: fs.readFileSync(path.join(repo, 'ui-shared/material-symbols/close.svg'), 'utf8'),
  };
  return source.slice(source.indexOf('<!DOCTYPE html>'), source.indexOf('</html>') + 7)
    .replaceAll('{{', '{').replaceAll('}}', '}')
    .replace(/(?<!\$)\{(\w+)\}/g, (match, name) => name in values ? values[name] : match);
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 375, height: 70 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    for (const dark of [true, false]) {
      await page.goto('about:blank');
      await page.setContent(documentHtml(dark, 375), { waitUntil: 'domcontentloaded' });
      await page.evaluate(async () => {
        await document.fonts.ready;
        document.body.classList.add('visible');
        barHeights.forEach((_, index) => barHeights[index] = 6 + (index * 17) % 48);
        updateState('paused', 0);
      });
      for (const shortcut of ['Esc / Page Down', 'Esc / Ctrl + Page Down', 'Esc']) {
        const text = hint.replace('{hotkey}', shortcut);
        const metrics = await page.evaluate(text => {
          updateSubtext(text);
          const element = document.querySelector('.sub-text');
          const range = document.createRange();
          range.selectNodeContents(element);
          return { text: element.textContent, rendered: range.getBoundingClientRect().width,
            available: element.clientWidth, axes: element.style.fontVariationSettings,
            size: getComputedStyle(element).fontSize };
        }, text);
        assert.equal(metrics.text, text);
        assert.ok(metrics.rendered <= metrics.available + 0.1, JSON.stringify(metrics));
        assert.equal(metrics.size, '10px');
        if (shortcut === 'Esc') assert.match(metrics.axes, /"wdth" 100/);
      }
      const before = await page.locator('#volume-canvas').evaluate(canvas => canvas.toDataURL());
      await page.waitForTimeout(100);
      assert.equal(await page.locator('#volume-canvas').evaluate(canvas => canvas.toDataURL()), before);
      await page.evaluate(text => updateSubtext(text), hint.replace('{hotkey}', 'Esc / Page Down'));
      const output = process.env.SGT_RECORDING_UI_EVIDENCE_DIR;
      if (output) {
        fs.mkdirSync(output, { recursive: true });
        await page.screenshot({ path: path.join(output, `recording-${dark ? 'dark' : 'light'}.png`) });
      }
    }
    assert.deepEqual(errors, []);
    console.log('PASS: variable-width hints fit in both themes at 10px; short hints reset; paused waveforms remain frozen.');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
