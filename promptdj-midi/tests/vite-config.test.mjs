import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

test('the bundle does not inline API keys from the build environment', () => {
  // The key reaches the page at runtime from the host (pm-dj-set-api-key); a
  // build-time define would only bake a developer's .env key into the bundle.
  const config = readFileSync(new URL('../vite.config.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(config, /process\.env\.[A-Z_]*API_KEY/);
  assert.doesNotMatch(config, /loadEnv/);
});
