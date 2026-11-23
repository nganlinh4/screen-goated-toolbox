import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { build } from 'vite';

const sharedInfoPath = /\sd="([^"]+)"/.exec(
  readFileSync(new URL('../../ui-shared/material-symbols/info.svg', import.meta.url), 'utf8'),
)[1];

test('the onboarding info glyph comes from the shared Material Symbols catalog', async () => {
  const source = readFileSync(new URL('../components/OnboardingPopup.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /<path\b|\sd="/, 'OnboardingPopup pastes SVG path data');

  const output = await build({
    root: fileURLToPath(new URL('..', import.meta.url)),
    configFile: false,
    logLevel: 'silent',
    build: {
      write: false,
      minify: false,
      lib: { entry: fileURLToPath(new URL('../components/OnboardingPopup.ts', import.meta.url)), formats: ['es'] },
      rolldownOptions: { external: [/^lit/] },
    },
  });
  const chunk = (Array.isArray(output) ? output[0] : output).output.find((item) => item.type === 'chunk');
  assert.ok(chunk.code.includes(sharedInfoPath), 'bundle carries the shared info path');
});
