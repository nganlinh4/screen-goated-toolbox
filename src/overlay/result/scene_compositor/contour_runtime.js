window.__SGT_PROCESSING_TIMELINE__ = function(entry, now, reduced) {
  const ease = t => { t = Math.max(0, Math.min(1, t)); return t * t * (3 - 2 * t); };
  // Critically damped travel with a C1 finite settle, not a truncated spring.
  // Outer edges lead the local contour; neither position nor velocity snaps.
  const duration = 900;
  const travel = (t, rate) => {
    t = Math.max(0, Math.min(1, t));
    if (t <= 0.7) return 1 - (1 + rate * t) * Math.exp(-rate * t);
    const start = 1 - (1 + rate * 0.7) * Math.exp(-rate * 0.7);
    const tangent = rate * rate * 0.7 * Math.exp(-rate * 0.7) * 0.3, u = (t - 0.7) / 0.3;
    return (2*u*u*u-3*u*u+1)*start + (u*u*u-2*u*u+u)*tangent + (3*u*u-2*u*u*u);
  };
  const progress = entry.morphAt == null ? 0 : reduced ? 1 : (now - entry.morphAt) / duration;
  const morph = travel(progress, 5), contract = travel(progress, 5.8);
  const fadeAt = Math.min(entry.closedAt ?? Infinity, entry.emptyAt ?? Infinity);
  const exitDuration = entry.finish || entry.emptyAt != null ? 360 : 220;
  const exit = ease((now - fadeAt) / exitDuration);
  const opacity = ease((now - entry.startedAt) / 160) * (1 - exit);
  const speed = !reduced && progress > 0 && progress < 1 ? 8 * progress * Math.exp(1 - 8 * progress) * (1 - ease(progress)) : 0;
  return { morph, contract, softness: reduced ? 0 : speed * 1.4 + exit * 1.2,
    retreat: reduced ? 0 : exit * 18, opacity,
    done: entry.closedAt != null && now >= fadeAt + exitDuration, phase: reduced ? 0 : (now - entry.startedAt) / 1800 };
};
window.__SGT_PROCESSING_CONTOURS__ = (function() {
  const effects = new Map(), reduced = matchMedia('(prefers-reduced-motion: reduce)');
  let frame = null, worker = null, workerUrl = null, busy = false;
  const queued = new Map();
  function dispatchGeometry() {
    if (busy || !queued.size) return;
    const [id, job] = queued.entries().next().value;
    queued.delete(id);
    const entry = effects.get(id);
    if (!entry || entry.done || !entry.gpu || entry.closedAt != null || entry.emptyAt != null) { dispatchGeometry(); return; }
    job.fixedPadding = entry.padding;
    try { workerForGeometry().postMessage(job); busy = true; }
    catch(error) { diagnostic(entry.id, 'field_worker_failed:' + String(error)); fallback(entry,error); }
  }
  function diagnostic(id, phase, duration = 0) {
    window.ipc.postMessage(JSON.stringify({ type: 'processing_diagnostic', id, phase, duration_ms: duration }));
  }
  function workerForGeometry() {
    if (worker) return worker;
    const source = window.__SGT_BUILD_PROCESSING_FIELD__.toString() + `
      onmessage = function(event) {
        const {id,revision,width,height,cells,fixedPadding}=event.data, started=performance.now();
        try { const field=buildProcessingField(width,height,cells,fixedPadding);
          postMessage({id,revision,field,duration:performance.now()-started},field?[field.pixels.buffer]:[]);
        } catch(error) { postMessage({id,revision,error:String(error)}); }
      };`;
    workerUrl = URL.createObjectURL(new Blob([source], { type: 'text/javascript' }));
    worker = new Worker(workerUrl);
    worker.onmessage = event => {
      busy = false;
      const result = event.data, entry = effects.get(String(result.id));
      // Accept monotonically newer fields while a newer destination is queued.
      // Discarding every superseded result can starve a busy streaming contour.
      if (entry && !entry.done && result.revision > entry.appliedRevision && entry.gpu
          && entry.closedAt == null && entry.emptyAt == null) {
        const now = performance.now();
        entry.pendingAt = null;
        if (result.error) { diagnostic(entry.id, 'field_failed:' + result.error); fallback(entry,result.error); }
        else if (result.field) {
          try {
            entry.gpu.upload(result.field, now);
            entry.appliedRevision = result.revision; entry.padding ??= result.field.padding;
            entry.morphAt ??= now;
            diagnostic(entry.id, 'field_ready', result.duration); schedule();
          } catch(error) { fallback(entry, error); }
        }
      }
      dispatchGeometry();
    };
    worker.onerror = () => {
      for (const entry of effects.values()) if (!entry.done && entry.gpu) fallback(entry,'field_worker_failed');
      stopWorker();
    };
    return worker;
  }
  function stopWorker() {
    worker?.terminate(); worker = null;
    busy = false; queued.clear();
    if (workerUrl) URL.revokeObjectURL(workerUrl); workerUrl = null;
  }
  function fallback(entry, error) {
    queued.delete(String(entry.id));
    entry.gpu?.destroy(); entry.gpu = null; entry.canvas.remove();
    entry.fallback?.destroy();
    entry.fallback = { element: window.__SGT_STATIC_PROCESSING_GLOW__(), destroy() { this.element.remove(); } };
    entry.viewport = null; position(entry);
    entry.element.appendChild(entry.fallback.element);
    diagnostic(entry.id, 'gpu_unavailable:' + String(error));
  }
  function create(effect) {
    const element = document.createElement('div'), canvas = document.createElement('canvas');
    element.className = 'processing-contour'; element.setAttribute('aria-hidden', 'true');
    element.style.cssText = 'position:absolute;pointer-events:none;overflow:hidden;z-index:2147480001';
    canvas.style.cssText = 'display:block;width:100%;height:100%;pointer-events:none';
    element.appendChild(canvas); document.body.appendChild(element);
    const entry = { id: effect.id, element, canvas, rect: effect.rect, revision: -1,
      startedAt: performance.now() - effect.elapsed_ms, morphAt: null, pendingAt: null,
      appliedRevision: -1, padding: null, emptyAt: null, closedAt: null, finish: false, done: false };
    position(entry);
    try { entry.gpu = window.__SGT_PROCESSING_GPU__(canvas); }
    catch (error) { fallback(entry, error); }
    canvas.addEventListener('webglcontextlost', event => {
      event.preventDefault(); if (!entry.done && entry.gpu) fallback(entry, 'context_lost');
    });
    return entry;
  }
  function position(entry) {
    const {rect, element, canvas} = entry, scale = devicePixelRatio || 1;
    const [x,y,width,height] = entry.viewport || [0,0,rect.width,rect.height];
    element.style.left = (rect.x+x) / scale + 'px'; element.style.top = (rect.y+y) / scale + 'px';
    element.style.width = width / scale + 'px'; element.style.height = height / scale + 'px';
    const reduction = Math.max(1, Math.sqrt(width * height / 4194304));
    const w = Math.max(1, Math.round(width / reduction)), h = Math.max(1, Math.round(height / reduction));
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
  }
  function destroy(entry) {
    entry.done = true;
    window.__SGT_PROCESSING_CONTROLS__?.remove(entry.id);
    queued.delete(String(entry.id));
    entry.gpu?.destroy(); entry.gpu = null; entry.fallback?.destroy(); entry.fallback = null;
    entry.element.remove(); entry.done = true;
  }
  function draw(now) {
    frame = null;
    let alive = false, animating = false;
    for (const entry of effects.values()) {
      if (entry.done) continue;
      const state = window.__SGT_PROCESSING_TIMELINE__(entry, now, reduced.matches);
      if (state.done) {
        destroy(entry);
        window.ipc.postMessage(JSON.stringify({ type: 'processing_finished', id: entry.id }));
        window.__SGT_BUTTON_SCENE__?.rebuild();
        continue;
      }
      alive = true;
      animating ||= now < entry.startedAt + 160 || (state.opacity > 0
        && (!reduced.matches || entry.closedAt != null || entry.emptyAt != null));
      entry.gpu?.draw(entry.rect.width, entry.rect.height, state.morph, state.phase, state.opacity,
        state.contract, state.softness, now, reduced.matches, state.retreat, entry.viewport,
        window.__SGT_PROCESSING_CONTROLS__?.opacity(entry.id, now, reduced.matches) ?? 0);
      if (entry.fallback) entry.element.style.opacity = String(state.opacity);
    }
    if (animating) schedule();
    if (!alive) stopWorker();
  }
  function schedule() { if (frame === null) frame = requestAnimationFrame(draw); }
  reduced.addEventListener('change', schedule);
  window.addEventListener('resize', () => { for (const entry of effects.values()) if (!entry.done) position(entry); schedule(); });
  return {
    wake: schedule,
    controlCutouts(id, boxes, work) {
      const entry = effects.get(String(id));
      if (!entry || entry.done) return;
      const scale = devicePixelRatio || 1;
      const controls = boxes.map(r => ({ x: (r.x-2)*scale-entry.rect.x, y: (r.y-2)*scale-entry.rect.y,
        w: (r.w+4)*scale, h: (r.h+4)*scale, radius: (r.radius+2)*scale }));
      const clip = work ? [work.x*scale-entry.rect.x,work.y*scale-entry.rect.y,
        (work.x+work.w)*scale-entry.rect.x,(work.y+work.h)*scale-entry.rect.y] : undefined;
      entry.gpu?.setControls(controls, clip);
      const x = Math.floor(Math.min(0,...controls.map(r=>r.x-2))), y = Math.floor(Math.min(0,...controls.map(r=>r.y-2)));
      entry.viewport = [x,y,Math.ceil(Math.max(entry.rect.width,...controls.map(r=>r.x+r.w+2)))-x,
        Math.ceil(Math.max(entry.rect.height,...controls.map(r=>r.y+r.h+2)))-y];
      if (entry.gpu) position(entry);
    },
    controlState(id) {
      return [...effects.values()].find(entry => !entry.done && String(entry.controlId) === String(id));
    },
    apply(snapshot) {
      const ids = new Set(snapshot.map(effect => String(effect.id)));
      for (const [id, entry] of effects) if (!ids.has(id)) { destroy(entry); effects.delete(id); }
      for (const effect of snapshot) {
        const id = String(effect.id);
        let entry = effects.get(id);
        if (!entry) { entry = create(effect); effects.set(id, entry); }
        if (entry.done) continue;
        entry.controlId = effect.control_id;
        if (effect.revision > entry.revision) {
          entry.revision = effect.revision;
          if (effect.revision > 0 && !effect.cells.length) {
            entry.emptyAt ??= performance.now(); queued.delete(id);
          } else if (effect.cells.length && entry.gpu && !effect.closing && entry.closedAt == null && entry.emptyAt == null) {
            entry.pendingAt ??= performance.now();
            queued.set(id, { id: effect.id, revision: effect.revision,
              width: effect.rect.width, height: effect.rect.height, cells: effect.cells });
          }
        }
        if (effect.closing && entry.closedAt === null) {
          entry.closedAt = performance.now(); entry.finish = effect.finish;
          queued.delete(id);
        }
      }
      if ([...effects.values()].some(entry => !entry.done)) { dispatchGeometry(); schedule(); }
      else { stopWorker(); if (frame !== null) cancelAnimationFrame(frame); frame = null; }
    }
  };
})();
