(() => {
  const pending = new Map();
  let nextRequest = 1;

  try {
    window.__SGT_CONTEXT__ = JSON.parse(window.CreationBridge.context());
  } catch {
    window.__SGT_CONTEXT__ = window.__SGT_CONTEXT__ || {};
  }
  window.invoke = (command, args = {}) => new Promise((resolve, reject) => {
    const id = String(nextRequest++);
    pending.set(id, { resolve, reject });
    window.CreationBridge.invoke(id, command, JSON.stringify(args ?? {}));
  });
  window.__sgtBridgeResolve = (id, ok, payload) => {
    const request = pending.get(String(id));
    if (!request) return;
    pending.delete(String(id));
    if (ok) request.resolve(payload == null ? null : JSON.parse(payload));
    else request.reject(new Error(payload || "Native bridge request failed"));
  };
  window.__sgtApplyContext = (payload) => {
    const context = JSON.parse(payload || "{}");
    window.__SGT_CONTEXT__ = context;
    window.applyHostContext?.(context);
  };
})();
