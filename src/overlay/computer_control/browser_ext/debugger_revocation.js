// A debugger Cancel is a local user revocation, independent of transport auth.
// Persist before admitting later work; extension reloads cannot clear the latch.
// Tab IDs restart with each browser session, so a new session retires the IDs it
// restored: they would otherwise pause unrelated tabs that reuse those numbers.
function createDebuggerRevocations(storage) {
  const KEY = "debuggerRevocationsV1";
  const MAX_TABS = 256;
  const revoked = new Set();
  const revokedThisSession = new Set();
  let loaded = false, unavailable = false, revision = 0;
  const validId = id => Number.isSafeInteger(id) && id > 0;
  const failure = code => Object.assign(new Error(code === "ERR_BROWSER_DEBUGGER_REVOKED"
    ? "Debugger access was cancelled. Resume this tab from the SGT extension popup."
    : "Debugger revocation storage is unavailable; browser control remains paused."), { code });
  const snapshot = tabs => ({ [KEY]: { version: 1, tabs: [...tabs] } });
  let serial = storage.get(KEY).then(values => {
    const record = values[KEY];
    if (record !== undefined) {
      if (!record || record.version !== 1 || !Array.isArray(record.tabs) || record.tabs.length > MAX_TABS || !record.tabs.every(validId)) throw new Error("invalid debugger revocations");
      for (const id of record.tabs) {
        if (!revoked.has(id) && revoked.size >= MAX_TABS) throw new Error("too many debugger revocations");
        revoked.add(id);
      }
    }
    if (revoked.size > MAX_TABS) throw new Error("too many debugger revocations");
    loaded = true;
  }).catch(() => { unavailable = true; });
  const enqueue = work => {
    const result = serial.then(work);
    serial = result.catch(() => { unavailable = true; });
    return result;
  };
  function assertAllowed(tabId) {
    if (!loaded || unavailable) throw failure("ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE");
    if (revoked.has(tabId)) throw failure("ERR_BROWSER_DEBUGGER_REVOKED");
  }
  return {
    async prepare(tabId) { await serial; assertAllowed(tabId); },
    assertAllowed,
    revoke(tabId) {
      if (!validId(tabId)) return Promise.resolve();
      if (!Number.isSafeInteger(revision + 1) || (!revoked.has(tabId) && revoked.size >= MAX_TABS)) {
        unavailable = true;
        return Promise.reject(failure("ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE"));
      }
      revision++;
      revoked.add(tabId);
      revokedThisSession.add(tabId);
      return enqueue(async () => {
        if (!loaded || unavailable) throw failure("ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE");
        await storage.set(snapshot(revoked));
      });
    },
    resume(tabId) {
      if (!validId(tabId)) return Promise.reject(failure("ERR_BROWSER_DEBUGGER_REVOKED"));
      return enqueue(async () => {
        if (!loaded || unavailable) throw failure("ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE");
        const owner = revision;
        const remaining = new Set(revoked);
        remaining.delete(tabId);
        await storage.set(snapshot(remaining));
        if (revision === owner) revoked.delete(tabId);
        else await storage.set(snapshot(revoked));
      });
    },
    retirePreviousBrowserSession() {
      return enqueue(async () => {
        if (!loaded || unavailable) throw failure("ERR_BROWSER_DEBUGGER_REVOCATION_STORAGE");
        const current = [...revoked].filter(id => revokedThisSession.has(id));
        if (current.length === revoked.size) return;
        await storage.set(snapshot(current));
        // A Cancel that arrived during the write is in revokedThisSession and
        // its own queued write persists it after this one.
        for (const id of [...revoked]) if (!revokedThisSession.has(id)) revoked.delete(id);
      });
    },
    forget(tabId) {
      const owner = revision;
      return enqueue(async () => {
        if (!loaded || unavailable || revision !== owner) return;
        const remaining = new Set(revoked);
        if (!remaining.delete(tabId)) return;
        await storage.set(snapshot(remaining));
        if (revision === owner) revoked.delete(tabId);
        else await storage.set(snapshot(revoked));
      });
    },
  };
}
