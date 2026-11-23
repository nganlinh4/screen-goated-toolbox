// Bind a physical activation to the session in which the gesture began.
(function() {
  const role = __SGT_REALTIME_ROLE__;
  const current = () => String(window.parent.realtimeSession?.() || '0');
  const pointers = new Map(), retiredPointers = new Map(), releasedPointers = new Map(), keys = new Map();
  let recentPointer = null, recentKey = null, compositionSession = null, retiredComposition = null, pointerTimer = null, keyTimer = null;
  function rememberPointer(session) {
    recentPointer = session;
    if (pointerTimer !== null) clearTimeout(pointerTimer);
    pointerTimer = setTimeout(() => { recentPointer = null; pointerTimer = null; }, 0);
  }
  function rememberKey(session) {
    recentKey = session;
    if (keyTimer !== null) clearTimeout(keyTimer);
    keyTimer = setTimeout(() => { recentKey = null; keyTimer = null; }, 0);
  }
  document.addEventListener('pointerdown', event => {
    if (event.isTrusted) {
      retiredPointers.delete(event.pointerId);
      releasedPointers.delete(event.pointerId);
      pointers.set(event.pointerId, current());
    }
  }, true);
  document.addEventListener('pointerup', event => {
    if (!event.isTrusted) return;
    const session = pointers.get(event.pointerId) ?? retiredPointers.get(event.pointerId);
    pointers.delete(event.pointerId);
    retiredPointers.delete(event.pointerId);
    if (session !== undefined) {
      releasedPointers.set(event.pointerId, session);
      while (releasedPointers.size > 64) releasedPointers.delete(releasedPointers.keys().next().value);
      rememberPointer(session);
    }
  }, true);
  document.addEventListener('pointercancel', event => {
    if (event.isTrusted) { pointers.delete(event.pointerId); retiredPointers.delete(event.pointerId); releasedPointers.delete(event.pointerId); }
  }, true);
  document.addEventListener('keydown', event => {
    if (!event.isTrusted) return;
    const key = event.code || event.key;
    if (!event.repeat || !keys.has(key)) keys.set(key, current());
    const origin = keys.get(key);
    rememberKey(origin);
    if (origin !== current()) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  }, true);
  document.addEventListener('keyup', event => {
    if (!event.isTrusted) return;
    const key = event.code || event.key, session = keys.get(key);
    keys.delete(key);
    if (session !== undefined) rememberKey(session);
  }, true);
  document.addEventListener('compositionstart', event => {
    if (event.isTrusted) { compositionSession = current(); retiredComposition = null; }
  }, true);
  document.addEventListener('compositionend', event => {
    if (!event.isTrusted) return;
    if (compositionSession !== null) { retiredComposition = compositionSession; rememberKey(compositionSession); }
    compositionSession = null;
  }, true);
  document.addEventListener('paste', event => { if (event.isTrusted) rememberKey(current()); }, true);
  for (const name of ['click', 'beforeinput', 'input', 'change']) {
    document.addEventListener(name, event => {
      if (!event.isTrusted) return;
      const activePointer = [...pointers.values()].at(-1) ?? null;
      const composing = event.isComposing || String(event.inputType || '').toLowerCase().includes('composition');
      const released = releasedPointers.get(event.pointerId) ?? null;
      const origin = name === 'click' ? (event.detail === 0 ? recentKey : (activePointer ?? released ?? recentPointer))
        : ((composing ? (compositionSession ?? retiredComposition) : null) ?? recentKey ?? activePointer ?? recentPointer);
      if (name === 'click' && event.detail !== 0) releasedPointers.delete(event.pointerId);
      if (origin !== null && origin !== current()) {
        event.preventDefault();
        event.stopImmediatePropagation();
      }
    }, true);
  }
  function retirePointers() {
    for (const [id, session] of pointers) retiredPointers.set(id, session);
    pointers.clear();
    while (retiredPointers.size > 64) retiredPointers.delete(retiredPointers.keys().next().value);
  }
  window.__SGT_REALTIME_SESSION_CHANGED__ = retirePointers;
  window.__SGT_CANCEL_REALTIME_POINTERS__ = retirePointers;
  window.addEventListener('blur', retirePointers);
  // The host sends the card text once (snapshot) and then only edits to it.
  let shownCommitted = '';
  window.setRealtimeText = (committed, draft) => {
    shownCommitted = committed;
    window.updateText(committed, draft);
  };
  window.editRealtimeText = (removeUtf16, append, draft) => {
    if (!Number.isSafeInteger(removeUtf16) || removeUtf16 < 0 || removeUtf16 > shownCommitted.length) {
      // A failed script restarts the renderer, which resynchronizes from a snapshot.
      throw new Error('realtime text edit does not match the shown text');
    }
    shownCommitted = shownCommitted.slice(0, shownCommitted.length - removeUtf16) + append;
    window.updateText(shownCommitted, draft);
  };
  window.REALTIME_COMPOSITOR_ROLE = role;
  window.realtimePostMessage = message => window.parent.realtimePostMessage(role, message);
})();
