// The service worker retains terminal evidence until the authenticated host
// acknowledges it. A reconnect may therefore settle a grant whose reply was
// lost; worker termination never fabricates a terminal result.
function createEffectLedger(send) {
  const records = new Map();
  const MAX_RECORDS = 256;
  const validToken = token => typeof token === "string" &&
    token.length > 0 && token.length <= 256 && /^[A-Za-z0-9_-]+$/.test(token);
  function report(record) {
    if (record.terminal) send({
      type: "effect_terminal", effectToken: record.token,
      effectMayHaveOccurred: record.effectMayHaveOccurred,
    });
  }
  return {
    reserve(token) {
      if (!validToken(token) || records.has(token) || records.size >= MAX_RECORDS) return null;
      const record = { token, terminal: false, effectMayHaveOccurred: false };
      records.set(token, record);
      return record;
    },
    restoreTerminal(token, effectMayHaveOccurred = true) {
      if (!validToken(token) || records.has(token) || records.size >= MAX_RECORDS) {
        throw new Error("invalid reload recovery token");
      }
      records.set(token, { token, terminal: true, effectMayHaveOccurred });
    },
    complete(record, effectMayHaveOccurred) {
      if (records.get(record.token) !== record || record.terminal) return;
      record.terminal = true;
      record.effectMayHaveOccurred = effectMayHaveOccurred;
      report(record);
    },
    acknowledge(token) {
      const record = records.get(token);
      if (record && record.terminal) records.delete(token);
    },
    isTerminal(token) {
      return records.get(token)?.terminal === true;
    },
    snapshot() {
      return { type: "effect_ledger_snapshot", active: [...records.values()]
        .filter(record => !record.terminal).map(record => record.token) };
    },
    replay() { for (const record of records.values()) report(record); },
  };
}

// A reload can destroy its own service worker before it emits a terminal
// event. Persist its token before admission; a new worker can then prove that
// the old worker cannot issue the call later, while conservatively treating
// the reload as possibly performed. Other effect types are never recovered
// from this marker.
function createReloadHandoff(storage, ledger) {
  const KEY = "effectReloadHandoffV11";
  let knownToken = null;
  let serial = Promise.resolve();
  const enqueue = work => {
    const result = serial.then(work);
    serial = result.catch(() => {});
    return result;
  };
  async function readMarker() {
    const values = await storage.get(KEY);
    const marker = values[KEY];
    if (marker === undefined) return null;
    if (!marker || typeof marker !== "object" || Array.isArray(marker) ||
        marker.version !== 1 || typeof marker.token !== "string" ||
        marker.token.length === 0 || marker.token.length > 256 ||
        !/^[A-Za-z0-9_-]+$/.test(marker.token)) {
      throw new Error("invalid reload recovery marker");
    }
    return marker;
  }
  const recovery = enqueue(async () => {
    const marker = await readMarker();
    if (marker) {
      ledger.restoreTerminal(marker.token);
      knownToken = marker.token;
    }
  }).then(() => null, error => error);
  async function removeOwned(token) {
    const marker = await readMarker();
    if (marker && marker.token === token) {
      await storage.remove(KEY);
      if (knownToken === token) knownToken = null;
    }
  }
  return {
    async ready() {
      const error = await recovery;
      if (error) throw error;
    },
    prepare(token) {
      return enqueue(async () => {
        const error = await recovery;
        if (error) throw error;
        if (await readMarker()) throw new Error("reload recovery is pending");
        await storage.set({ [KEY]: { version: 1, token } });
        knownToken = token;
      });
    },
    abandon(token) { return enqueue(() => removeOwned(token)); },
    acknowledge(token) {
      return enqueue(async () => {
        if (knownToken === token) await removeOwned(token);
      });
    },
  };
}

// A lease is connection-scoped until an effect starts. Once started, its
// terminal record survives channel close until the operation actually settles.
function createCdpAdmission(isCurrent, send, ledger) {
  const jobs = new Map();
  const MAX_JOBS = 128;
  const MAX_TIMEOUT_MS = 15000;
  const now = () => performance.now();

  function accept(msg) {
    if (!Number.isSafeInteger(msg.id) || msg.id <= 0 || jobs.has(msg.id) || jobs.size >= MAX_JOBS ||
        !Number.isInteger(msg.timeoutMs) || msg.timeoutMs <= 0 || msg.timeoutMs > MAX_TIMEOUT_MS) return null;
    const record = ledger.reserve(msg.effectToken);
    if (!record) return null;
    let wakeGrant = null;
    let wakeRevoked = null;
    const revoked = new Promise(resolve => { wakeRevoked = resolve; });
    const job = { id: msg.id, deadline: now() + msg.timeoutMs, cancelled: false, granted: false, started: false, deferred: false };
    const valid = () => !job.cancelled && isCurrent() && now() < job.deadline;
    const revoke = () => {
      if (job.cancelled) return;
      job.cancelled = true;
      if (wakeGrant) { const resolve = wakeGrant; wakeGrant = null; resolve(false); }
      wakeRevoked();
    };
    let timer = setTimeout(revoke, msg.timeoutMs);
    job.valid = valid;
    job.revoke = revoke;
    job.wait = async promise => {
      if (!valid()) throw new Error("request expired before effect");
      const result = await Promise.race([
        Promise.resolve(promise).then(value => ({ value })),
        revoked.then(() => ({ revoked: true })),
      ]);
      if (result.revoked || !valid()) throw new Error("request expired before effect");
      return result.value;
    };
    job.grant = remainingMs => {
      if (!wakeGrant) return;
      if (!Number.isInteger(remainingMs) || remainingMs <= 0 || remainingMs > MAX_TIMEOUT_MS) { revoke(); return; }
      job.deadline = Math.min(job.deadline, now() + remainingMs);
      clearTimeout(timer);
      timer = setTimeout(revoke, Math.max(0, job.deadline - now()));
      const resolve = wakeGrant; wakeGrant = null;
      job.granted = valid();
      resolve(job.granted);
    };
    job.requestGrant = async () => {
      if (!valid() || wakeGrant || job.granted) return false;
      const result = new Promise(resolve => { wakeGrant = resolve; });
      try {
        await job.wait(send({ type: "cdp_dispatch_ready", requestId: job.id }));
        return await job.wait(result);
      } catch (_) { revoke(); return false; }
    };
    // Check and invoke without an await between them.
    job.invoke = action => {
      if (!job.granted || !valid()) throw new Error("request expired before effect");
      job.started = true;
      return action();
    };
    job.deferTerminal = () => { job.deferred = true; };
    job.settleDeferred = () => ledger.complete(record, true);
    job.complete = () => {
      if (jobs.get(job.id) === job) jobs.delete(job.id);
      clearTimeout(timer);
      revoke();
      if (!job.deferred) ledger.complete(record, job.started);
    };
    jobs.set(job.id, job);
    return job;
  }

  return {
    accept,
    control(msg) {
      const job = jobs.get(msg.requestId);
      if (!job) return;
      if (msg.type === "request_cancel") job.revoke();
      else if (msg.type === "cdp_dispatch_grant") job.grant(msg.remainingMs);
    },
    finish(job) { job.complete(); },
    close() {
      for (const job of [...jobs.values()]) {
        job.revoke();
        if (!job.started) job.complete();
      }
    },
  };
}
