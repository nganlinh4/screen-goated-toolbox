const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function scripts(file) {
  const source = fs.readFileSync(path.join(__dirname, file), 'utf8');
  return [...source.matchAll(/let script = format!\(\s*r#"([\s\S]*?)"#\s*\);/g)].map(match => match[1]);
}

const [activation] = scripts('activation.rs');
const [apply, revoke] = scripts('editor_text.rs');
assert.ok(activation && apply && revoke, 'production page scripts must be present');

const editor = { value: '', selectionStart: 0, selectionEnd: 0, focusCount: 0, focus() { this.focusCount++; } };
const window = { __sgtDocumentNonce: 'doc', __sgtCancelledThrough: '0' };
const context = vm.createContext({ window, document: { getElementById: () => editor } });
function run(template, payload) {
  const source = template.replace('{payload}', JSON.stringify(payload)).replaceAll('{{', '{').replaceAll('}}', '}');
  return vm.runInContext(source, context);
}
function activate(session) { return run(activation, { nonce: 'doc', session }); }
function edit(session, sequence, text = '') {
  return run(apply, { nonce: 'doc', session, id: `op-${sequence}`, sequence, text, action: 'insert', focus: false });
}
function cancel(session, sequence) {
  return run(revoke, { nonce: 'doc', session, id: `op-${sequence}`, sequence });
}

assert.equal(activate('1').status, 'ready');
assert.equal(cancel('1', '1').status, 'revoked');
assert.equal(edit('1', '1', 'stale').status, 'rejected');
assert.equal(edit('1', '2', 'one').status, 'applied');
assert.equal(editor.value, 'one');
assert.equal(activate('1').status, 'ready');
assert.equal(editor.value, 'one', 'retrying an acknowledged activation must not clear text');
assert.equal(window.__sgtCancelledThrough, '1', 'same-session retry retains cancellation');
assert.equal(activate('3').status, 'ready');
assert.equal(window.__sgtCancelledThrough, '0');
assert.equal(editor.value, '');
assert.equal(cancel('1', '99').status, 'revoked');
assert.equal(window.__sgtCancelledThrough, '0', 'old-session revoke cannot affect new session');
assert.equal(edit('3', '1', 'fresh').status, 'applied');
assert.equal(editor.value, 'fresh');
assert.equal(edit('3', '2', '__REPLACE_ALL__literal').status, 'applied');
assert.equal(editor.value, 'fresh__REPLACE_ALL__literal');
assert.equal(activate('1').status, 'stale');
assert.equal(editor.value, 'fresh__REPLACE_ALL__literal');
const wrongDocument = run(apply, { nonce: 'other', session: '3', id: 'x', sequence: '3', text: 'bad', action: 'replace', focus: false });
assert.equal(wrongDocument.status, 'rejected');
assert.equal(wrongDocument.nonce, 'doc');
assert.equal(editor.value, 'fresh__REPLACE_ALL__literal');
const submittedRevision = window.__sgtTextRevision;
editor.value += ' newer';
window.__sgtTextRevision = (BigInt(window.__sgtTextRevision) + 1n).toString();
const staleClear = run(apply, { nonce: 'doc', session: '3', id: 'clear-old', sequence: '4', action: 'clear', revision: submittedRevision, passive: false, snapshot: '', focus: false });
assert.equal(staleClear.status, 'skipped');
assert.equal(editor.value, 'fresh__REPLACE_ALL__literal newer');
const focused = run(apply, { nonce: 'doc', session: '3', id: 'focus', sequence: '5', action: 'focus', focus: true });
assert.equal(focused.status, 'applied');
assert.equal(editor.focusCount, 1);
window.__sgtPassiveRevision = '9';
editor.value = 'inserted plus';
const latePassiveInsert = run(apply, { nonce: 'doc', session: '3', id: 'passive-old', sequence: '9', action: 'insert', text: 'inserted', focus: false, nativePassive: true, nativeSnapshot: 'inserted', nativeRevision: '8' });
assert.equal(latePassiveInsert.status, 'applied');
assert.equal(editor.value, 'inserted plus', 'late renderer ACK must not overwrite newer native input');
editor.value = 'passive snapshot';
window.__sgtPassiveRevision = '7';
const passiveClear = run(apply, { nonce: 'doc', session: '3', id: 'clear-passive', sequence: '6', action: 'clear', revision: '7', passive: true, snapshot: 'passive snapshot', focus: false });
assert.equal(passiveClear.status, 'applied');
assert.equal(editor.value, '');
assert.equal(window.__sgtPassiveRevision, '8');
editor.value = 'same';
window.__sgtTextRevision = '12';
const sameTextNewRevision = run(apply, { nonce: 'doc', session: '3', id: 'clear-same', sequence: '7', action: 'clear', revision: '11', passive: false, snapshot: '', focus: false });
assert.equal(sameTextNewRevision.status, 'skipped');
assert.equal(editor.value, 'same');
assert.equal(cancel('3', '8').status, 'revoked');
const cancelledFocus = run(apply, { nonce: 'doc', session: '3', id: 'focus-old', sequence: '8', action: 'focus', focus: true });
assert.equal(cancelledFocus.status, 'rejected');
assert.equal(editor.focusCount, 1);
console.log('Text-input delivery page protocol passed');
