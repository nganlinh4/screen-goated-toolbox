// A canceled preparation must not leave a new debugger attachment behind. All
// concurrent waiters share one claim: a successful waiter retains the session.
async function acquireAttachment(tabId, wait, valid) {
  let claim = attachmentInterests.get(tabId);
  if (!claim) {
    claim = { waiters: new Set(), retained: false, complete: false, rootReady: false,
      initialRegistry: attached.has(tabId) ? frameRegistries.get(tabId) : null, registry: null };
    attachmentInterests.set(tabId, claim);
    claim.promise = ensureAttached(tabId).then(() => {
      claim.complete = true;
      claim.registry = frameRegistries.get(tabId);
      if (claim.registry && claim.registry === claim.initialRegistry) claim.retained = true;
      scheduleAttachmentCleanup(tabId, claim);
    }, error => {
      claim.complete = true;
      if (!claim.rootReady) claim.registry = frameRegistries.get(tabId);
      if (claim.registry && claim.registry === claim.initialRegistry) claim.retained = true;
      scheduleAttachmentCleanup(tabId, claim);
      throw error;
    });
  }
  const waiter = { valid };
  claim.waiters.add(waiter);
  try {
    await wait(claim.promise);
    if (!valid()) throw new Error("attachment request expired");
    let released = false;
    return {
      retain() { if (!released) claim.retained = true; },
      release() {
        if (released) return;
        released = true;
        claim.waiters.delete(waiter);
        scheduleAttachmentCleanup(tabId, claim, true);
      },
    };
  } catch (error) {
    claim.waiters.delete(waiter);
    scheduleAttachmentCleanup(tabId, claim, true);
    throw error;
  }
}

// Chrome keeps a debugger session when this extension's worker restarts and then
// rejects attach from the replacement worker. Detach succeeds only for this
// extension's own session, so it proves the leftover session was ours to replace.
async function attachRootDebugger(tabId, registry, mayReclaim) {
  try {
    await chrome.debugger.attach({ tabId }, "1.3");
  } catch (error) {
    if (!mayReclaim) throw error;
    try { await chrome.debugger.detach({ tabId }); } catch (_) { throw error; }
    if (frameRegistries.get(tabId) !== registry) throw error;
    debuggerRevocations.assertAllowed(tabId);
    await chrome.debugger.attach({ tabId }, "1.3");
  }
}

function scheduleAttachmentCleanup(tabId, claim, immediate = false) {
  if (!claim.complete && !claim.rootReady) return;
  if (claim.retained && !claim.waiters.size) {
    if (attachmentInterests.get(tabId) === claim) attachmentInterests.delete(tabId);
    return;
  }
  const cleanup = () => {
    if (attachmentInterests.get(tabId) !== claim) return;
    if ([...claim.waiters].some(waiter => waiter.valid())) return;
    attachmentInterests.delete(tabId);
    if (!claim.retained) {
      // Do not let a stalled setup promise prevent a later owner from attaching.
      attaching.delete(tabId);
      void detachOwned(tabId, claim.registry);
    }
  };
  if (immediate) cleanup(); else setTimeout(cleanup, 0);
}

async function handleFrames(msg, reply) {
  let job;
  let timer;
  try {
    if (!Number.isInteger(msg.tabId) || msg.tabId <= 0) throw new Error("frame inventory requires a positive tab id");
    if (inventoryJobs.has(msg.tabId) || inventoryJobs.size >= MAX_INVENTORY_JOBS) throw new Error("frame inventory already in progress");
    job = { cancelled: false, deadline: Date.now() + FRAME_SETTLE_TIMEOUT_MS };
    job.revoked = new Promise(resolve => { job.revoke = resolve; });
    inventoryJobs.set(msg.tabId, job);
    const work = (async () => {
      if (msg.requireActive && msg.tabId !== await inventoryStep(job, () => activeTabId())) throw new Error("target tab is no longer active");
      job.attachment = await acquireAttachment(msg.tabId, promise => inventoryStep(job, () => Promise.race([
        promise, job.revoked.then(() => { throw new Error("frame inventory expired"); }),
      ])),
        () => !job.cancelled && Date.now() < job.deadline);
      const coverage = await completeFrameInventory(msg.tabId, job);
      if (msg.requireActive && msg.tabId !== await inventoryStep(job, () => activeTabId())) throw new Error("target tab changed during frame inventory");
      const { registry, generation, sessions } = coverage;
      debuggerRevocations.assertAllowed(msg.tabId);
      if (registry.incomplete || registry.needsReconciliation || registry.pending.size || registry.generation !== generation || frameRegistries.get(msg.tabId) !== registry) {
        throw new Error("frame inventory changed before delivery");
      }
      checkInventory(job);
      job.attachment.retain();
      return { registryId: registry.id, generation, sessions };
    })();
    work.then(() => { if (inventoryJobs.get(msg.tabId) === job) inventoryJobs.delete(msg.tabId); },
      () => { if (inventoryJobs.get(msg.tabId) === job) inventoryJobs.delete(msg.tabId); });
    const result = await Promise.race([
      work,
      new Promise((_, reject) => { timer = setTimeout(() => { job.cancelled = true; job.revoke(); if (job.attachment) job.attachment.release(); reject(new Error("frame inventory timed out")); }, FRAME_SETTLE_TIMEOUT_MS); }),
    ]);
    reply({ id: msg.id, ok: true, result });
  } catch (e) {
    reply({ id: msg.id, ok: false, code: e?.code || "ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE", error: String(e && e.message ? e.message : e) });
  } finally {
    if (timer) clearTimeout(timer);
    if (job?.attachment) job.attachment.release();
  }
}

function checkInventory(job) {
  if (job.cancelled || Date.now() >= job.deadline) throw new Error("frame inventory expired");
}

async function inventoryStep(job, action) {
  checkInventory(job);
  const result = await action();
  checkInventory(job);
  return result;
}

// A page beyond the frame bound is refused as a whole: partial coverage would
// let absence and no-effect claims rest on frames that were never observed.
function tooManyFrames() {
  return Object.assign(new Error(`page has more than ${MAX_CHILD_FRAMES} frames`), { code: "ERR_BROWSER_TOO_MANY_FRAMES" });
}

async function completeFrameInventory(tabId, job) {
  const registry = frameRegistries.get(tabId);
  if (!registry) throw new Error("frame inventory unavailable");
  await inventoryStep(job, () => settleFrameRegistry(registry));
  if (registry.overCapacity && frameRegistries.get(tabId) === registry) throw tooManyFrames();
  if (registry.incomplete || registry.needsReconciliation || frameRegistries.get(tabId) !== registry) throw new Error("frame inventory incomplete");
  const generation = registry.generation;
  await verifyFrameCoverage(tabId, registry, job);
  if (registry.incomplete || registry.needsReconciliation || registry.pending.size || registry.generation !== generation || frameRegistries.get(tabId) !== registry) {
    throw new Error("frame inventory changed during coverage check");
  }
  return { registry, generation, sessions: [...registry.sessions.keys()] };
}

async function verifyFrameCoverage(tabId, registry, job) {
  let frameCount = 0;
  for (const sessionId of [null, ...registry.sessions.keys()]) {
    const target = sessionId ? { tabId, sessionId } : { tabId };
    const tree = await inventoryStep(job, () => debuggerCommand(target, "Page.getFrameTree", {}));
    if (typeof tree?.frameTree?.frame?.id !== "string" || !tree.frameTree.frame.id ||
        (tree.frameTree.childFrames != null && (!Array.isArray(tree.frameTree.childFrames) || tree.frameTree.childFrames.length))) {
      throw new Error("same-process frame is not observable");
    }
    const document = await inventoryStep(job, () => debuggerCommand(target, "DOM.getDocument", { depth: 0 }));
    const nodeId = document?.root?.nodeId;
    if (!Number.isInteger(nodeId) || nodeId <= 0) throw new Error("frame document unavailable");
    checkInventory(job);
    const search = await debuggerCommand(target, "DOM.performSearch", { query: "iframe, frame", includeUserAgentShadowDOM: false });
    try {
      checkInventory(job);
      if (typeof search?.searchId !== "string" || !Number.isInteger(search.resultCount) || search.resultCount < 0) throw new Error("frame owner search unavailable");
      frameCount += search.resultCount;
      if (frameCount > MAX_CHILD_FRAMES) throw tooManyFrames();
      const expected = new Set([...registry.sessions.values()].filter(info => info.parent === sessionId).map(info => info.targetId));
      const nodes = search.resultCount ? await inventoryStep(job, () => debuggerCommand(target, "DOM.getSearchResults", { searchId: search.searchId, fromIndex: 0, toIndex: search.resultCount })) : { nodeIds: [] };
      if (!Array.isArray(nodes?.nodeIds) || nodes.nodeIds.length !== search.resultCount) throw new Error("frame owners unavailable");
      for (const owner of nodes.nodeIds) {
        const described = await inventoryStep(job, () => debuggerCommand(target, "DOM.describeNode", { nodeId: owner, depth: 0 }));
        const node = described?.node;
        if (!['IFRAME', 'FRAME'].includes(node?.nodeName)) continue;
        if (typeof node.frameId !== "string" || !expected.delete(node.frameId)) throw new Error("same-process or unattached frame is not observable");
      }
      if (expected.size) throw new Error("attached frame has no current owner");
    } finally {
      if (typeof search?.searchId === "string") {
        try { await debuggerCommand(target, "DOM.discardSearchResults", { searchId: search.searchId }); } catch (_) {}
      }
    }
  }
}

async function settleFrameRegistry(registry) {
  // Each child attach may discover further children before its promise settles.
  while (registry.pending.size) await Promise.all([...registry.pending]);
}

function setAutoAttach(tabId, sessionId, registry) {
  const target = sessionId ? { tabId, sessionId } : { tabId };
  const operation = (async () => {
    try {
      await debuggerCommand(target, "Target.setAutoAttach", {
        autoAttach: true, waitForDebuggerOnStart: false, flatten: true,
      });
    } catch (_) {
      if (frameRegistries.get(tabId) === registry && (!sessionId || registry.sessions.has(sessionId))) registry.incomplete = true;
    }
  })();
  registry.pending.add(operation);
  operation.finally(() => registry.pending.delete(operation));
  return operation;
}

function recordFrameEvent(source, method, params) {
  const tabId = source.tabId;
  const registry = frameRegistries.get(tabId);
  if (!registry) return;
  const sessionId = params?.sessionId;
  if (typeof sessionId !== "string" || !sessionId) return;
  if (method === "Target.attachedToTarget" && params.targetInfo?.type === "iframe") {
    if (registry.sessions.has(sessionId)) return;
    if (typeof params.targetInfo.targetId !== "string" || !params.targetInfo.targetId || sessionId.length > 1024) {
      registry.incomplete = true; return;
    }
    if ([...registry.sessions.values()].some(info => info.targetId === params.targetInfo.targetId)) {
      registry.incomplete = true; return;
    }
    if (registry.sessions.size >= MAX_CHILD_FRAMES) { registry.incomplete = true; registry.overCapacity = true; return; }
    registry.sessions.set(sessionId, { parent: source.sessionId || null, targetId: params.targetInfo.targetId });
    registry.generation++;
    setAutoAttach(tabId, sessionId, registry);
  } else if (method === "Target.detachedFromTarget" && registry.sessions.has(sessionId)) {
    const retired = new Set([sessionId]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const [child, info] of registry.sessions) {
        if (info.parent && retired.has(info.parent) && !retired.has(child)) { retired.add(child); changed = true; }
      }
    }
    for (const child of retired) registry.sessions.delete(child);
    registry.generation++;
  }
}
