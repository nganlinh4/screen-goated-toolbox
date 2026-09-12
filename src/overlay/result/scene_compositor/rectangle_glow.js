// Device-loss feedback is static; all animated outlines use the same GPU light.
window.__SGT_STATIC_PROCESSING_GLOW__ = function() {
  const element = document.createElement('div');
  element.setAttribute('aria-hidden', 'true');
  element.style.cssText = 'position:absolute;inset:0;pointer-events:none;border-radius:28px;'
    + 'box-shadow:inset 0 0 0 2px white,inset 0 0 14px 3px #8870ff';
  return element;
};
window.__SGT_CREATE_RECTANGLE_GLOW__ = function() {
  const element = document.createElement('div');
  element.className = 'processing-glow'; element.setAttribute('aria-hidden', 'true');
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  let canvas = null, gpu = null, frame = null, active = false, destroyed = false;
  let width = 1, height = 1, started = 0, closed = null;
  const ease = t => { t = Math.max(0,Math.min(1,t)); return t*t*(3-2*t); };
  function schedule() { if (frame === null && !destroyed) frame = requestAnimationFrame(draw); }
  function release() {
    gpu?.destroy(); gpu = null; canvas = null; element.replaceChildren();
  }
  function unavailable() {
    release(); element.appendChild(window.__SGT_STATIC_PROCESSING_GLOW__());
  }
  function allocate() {
    canvas = document.createElement('canvas');
    canvas.style.cssText = 'width:100%;height:100%;display:block;pointer-events:none';
    resizeBuffer(); element.appendChild(canvas);
    try { gpu = window.__SGT_PROCESSING_GPU__(canvas); }
    catch (_) { unavailable(); }
    canvas?.addEventListener('webglcontextlost', event => {
      event.preventDefault(); if (!destroyed) { unavailable(); schedule(); }
    });
  }
  function resizeBuffer() {
    if (!canvas) return;
    const reduction = Math.max(1,Math.sqrt(width*height/4194304));
    const w = Math.max(1,Math.round(width/reduction)), h = Math.max(1,Math.round(height/reduction));
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
  }
  function draw(now) {
    frame = null;
    const alpha = ease((now-started)/160)*(closed == null ? 1 : 1-ease((now-closed)/160));
    element.style.opacity = String(alpha);
    gpu?.draw(width,height,0,reduced.matches ? 0 : (now-started)/1800,1,0,0,now,reduced.matches);
    if (closed != null && now >= closed+160) { release(); return; }
    if ((!reduced.matches && gpu) || now < started+160 || closed != null) schedule();
  }
  reduced.addEventListener('change', schedule);
  return {
    element,
    resize(w,h,scale) { width = Math.max(1,w*scale); height = Math.max(1,h*scale); resizeBuffer(); if (active) schedule(); },
    setState(value) {
      if (destroyed || active === value) return;
      active = value;
      if (value) {
        started = performance.now(); closed = null;
        if (!element.childElementCount) allocate();
      } else closed = performance.now();
      schedule();
    },
    destroy() {
      destroyed = true; if (frame !== null) cancelAnimationFrame(frame);
      reduced.removeEventListener('change', schedule); release();
    }
  };
};
