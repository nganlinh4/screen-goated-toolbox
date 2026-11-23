import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { discoverTests, runTests } from '../test-javascript.mjs';

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'sgt-javascript-tests-'));
  const directories = ['src', 'scripts', 'scripts/tests', 'tests', 'src/nested', 'src/dist'];
  for (const directory of directories) fs.mkdirSync(path.join(root, directory));
  const files = new Map();
  t.after(() => {
    assert.equal(path.dirname(fs.realpathSync(root)), fs.realpathSync(os.tmpdir()));
    for (const [file, content] of files) {
      assert.equal(fs.readFileSync(file, 'utf8'), content);
      fs.unlinkSync(file);
    }
    for (const directory of [...directories].reverse()) fs.rmdirSync(path.join(root, directory));
    fs.rmdirSync(root);
  });
  return {root, write(name, content = '') {
    const file = path.join(root, name);
    assert.ok(file.startsWith(root + path.sep));
    fs.writeFileSync(file, content, {flag: 'wx'});
    files.set(file, content);
    return file;
  }};
}

test('new nested tests are discovered with explicit browser requirements and generated trees excluded', t => {
  const h = fixture(t);
  const ordinary = ['src/a.test.cjs', 'src/nested/new.test.mjs', 'src/nested/new.test.ts', 'scripts/tests/x.test.js', 'tests/z.test.cjs'];
  for (const file of ordinary) h.write(file);
  h.write('src/contract.browser.test.cjs');
  h.write('src/dist/generated.test.cjs');
  const relative = suite => discoverTests(h.root, suite).map(file => path.relative(h.root, file).replaceAll('\\', '/'));
  assert.deepEqual(relative('unit'), [...ordinary].sort());
  assert.deepEqual(relative('browser'), ['src/contract.browser.test.cjs']);
  assert.equal(relative('all').length, ordinary.length + 1);
  assert.throws(() => discoverTests(h.root, 'unknown'), /Unknown JavaScript suite/);
});

test('the real child test process propagates failure and empty selection cannot report success', t => {
  const h = fixture(t);
  const passing = h.write('src/pass.test.cjs', "require('node:assert/strict').equal(1, 1);\n");
  const failing = h.write('src/fail.test.cjs', "require('node:assert/strict').equal(1, 2);\n");
  assert.equal(runTests([passing], h.root, 'pipe'), 0);
  assert.notEqual(runTests([failing], h.root, 'pipe'), 0);
  assert.throws(() => runTests([], h.root, 'pipe'), /No JavaScript tests/);
});
