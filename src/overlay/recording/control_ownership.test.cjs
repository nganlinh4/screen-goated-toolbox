const { test } = require('node:test');
const assert = require('node:assert/strict');
const { createRecordingControlOwner } = require('./control_ownership.js');

test('old pointer and keyboard activations retain their session across replacement', () => {
    const owner = createRecordingControlOwner();
    const pause = {};
    const cancel = {};
    owner.setSession('1');
    owner.capturePointer(cancel);
    owner.captureKey(pause);
    owner.setSession('2');
    assert.equal(owner.consume(cancel, false), '1');
    assert.equal(owner.consume(pause, true), '1');
    owner.capturePointer(cancel);
    assert.equal(owner.consume(cancel, false), '2');
});

test('uncorrelated accessibility activation uses current session', () => {
    const owner = createRecordingControlOwner();
    const button = {};
    owner.setSession('2');
    assert.equal(owner.consume(button, true), '2');
    owner.capturePointer(button);
    assert.equal(owner.consume({}, false), '');
});
