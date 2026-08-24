(function() {
  let active = null;

  function geometry(resize) {
    const scale = window.devicePixelRatio || 1;
    const minWidth = __SGT_MIN_WINDOW_WIDTH_PX__ / scale;
    const minHeight = __SGT_MIN_WINDOW_HEIGHT_PX__ / scale;
    const west = resize.edge.includes('w'); const east = resize.edge.includes('e');
    const north = resize.edge.includes('n'); const south = resize.edge.includes('s');
    let x = resize.rect.x; let y = resize.rect.y;
    let width = resize.rect.width; let height = resize.rect.height;
    if (east) width = Math.max(minWidth, width + resize.dx);
    if (south) height = Math.max(minHeight, height + resize.dy);
    if (west) {
      width = Math.max(minWidth, width - resize.dx);
      x = resize.rect.x + resize.rect.width - width;
    }
    if (north) {
      height = Math.max(minHeight, height - resize.dy);
      y = resize.rect.y + resize.rect.height - height;
    }
    return { x: x, y: y, width: width, height: height };
  }

  function render() {
    const resize = active; if (!resize) return;
    resize.frame = 0;
    const rect = geometry(resize);
    resize.entry.card.style.transform = 'translate3d(' + rect.x + 'px,' + rect.y + 'px,0)';
    resize.entry.card.style.width = rect.width + 'px';
    resize.entry.card.style.height = rect.height + 'px';
    resize.entry.processing.resize(rect.width, rect.height, window.devicePixelRatio || 1);
  }

  function start(event, entry, edge) {
    if (event.button !== 0 || active) return;
    event.preventDefault(); event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    const rect = entry.card.getBoundingClientRect();
    active = {
      entry: entry, edge: edge, pointerId: event.pointerId,
      startX: event.clientX, startY: event.clientY, dx: 0, dy: 0, frame: 0,
      rect: { x: rect.left, y: rect.top, width: rect.width, height: rect.height }
    };
    window.ipc.postMessage(JSON.stringify({
      action: 'result_resize_start', hwnd: entry.card.dataset.id, edge: edge
    }));
    window.__SGT_BUTTON_SCENE__?.setDragActive(true);
  }

  function update(event) {
    if (!active || event.pointerId !== active.pointerId) return;
    active.dx = event.clientX - active.startX;
    active.dy = event.clientY - active.startY;
    if (!active.frame) active.frame = requestAnimationFrame(render);
    event.preventDefault();
  }

  function finish(event) {
    const resize = active;
    if (!resize || (event && event.pointerId !== resize.pointerId)) return;
    if (event) {
      resize.dx = event.clientX - resize.startX;
      resize.dy = event.clientY - resize.startY;
    }
    if (resize.frame) cancelAnimationFrame(resize.frame);
    render();
    const scale = window.devicePixelRatio || 1;
    window.ipc.postMessage(JSON.stringify({
      action: 'result_resize_finish', hwnd: resize.entry.card.dataset.id, edge: resize.edge,
      dx: Math.round(resize.dx * scale), dy: Math.round(resize.dy * scale)
    }));
    active = null;
    window.__SGT_BUTTON_SCENE__?.setDragActive(false);
  }

  function attach(entry) {
    const handles = [];
    for (const edge of ['n', 's', 'e', 'w', 'nw', 'ne', 'sw', 'se']) {
      const handle = document.createElement('div');
      handle.className = 'resize-handle'; handle.dataset.edge = edge;
      handle.addEventListener('pointerdown', event => start(event, entry, edge));
      entry.card.appendChild(handle); handles.push(handle);
    }
    return { destroy: function() {
      if (active && active.entry === entry) {
        if (active.frame) cancelAnimationFrame(active.frame);
        active = null;
        window.__SGT_BUTTON_SCENE__?.setDragActive(false);
      }
      for (const handle of handles) handle.remove();
    }};
  }

  document.addEventListener('pointermove', update, true);
  document.addEventListener('pointerup', finish, true);
  document.addEventListener('pointercancel', finish, true);
  window.addEventListener('blur', () => finish(null));
  document.addEventListener('visibilitychange', () => { if (document.hidden) finish(null); });
  window.__SGT_CARD_RESIZE__ = { attach: attach };
})();
