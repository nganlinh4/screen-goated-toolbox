function createRecordingControlOwner() {
    let session = '';
    let pointer = null;
    let key = null;
    return {
        setSession(value) { session = value; },
        capturePointer(button) { pointer = { button, session }; },
        captureKey(button) { key = { button, session }; },
        consume(button, keyboard) {
            const owner = keyboard ? key : pointer;
            if (keyboard) key = null; else pointer = null;
            if (!owner) return session;
            return owner.button === button ? owner.session : '';
        },
    };
}

if (typeof module !== 'undefined') module.exports = { createRecordingControlOwner };
