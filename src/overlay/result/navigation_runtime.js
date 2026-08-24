(function() {
  const edgeSize = __SGT_NAVIGATION_RESIZE_EDGE_PX__;

  function installResizeEdges() {
    if (!document.documentElement || document.querySelector('[data-sgt-navigation-resize]')) return;
    const host = document.createElement('div');
    host.dataset.sgtNavigationResize = '1';
    host.style.cssText = 'all:initial!important;position:fixed!important;inset:0!important;' +
      'z-index:2147483647!important;pointer-events:none!important;contain:strict!important';
    const shadow = host.attachShadow({ mode: 'closed' });
    shadow.innerHTML = `<style>
      :host{all:initial}
      [data-edge]{position:fixed;display:block;pointer-events:auto;touch-action:none;background:transparent}
      [data-edge="n"],[data-edge="s"]{left:${edgeSize}px;right:${edgeSize}px;height:${edgeSize}px;cursor:ns-resize}
      [data-edge="n"]{top:0}[data-edge="s"]{bottom:0}
      [data-edge="e"],[data-edge="w"]{top:${edgeSize}px;bottom:${edgeSize}px;width:${edgeSize}px;cursor:ew-resize}
      [data-edge="e"]{right:0}[data-edge="w"]{left:0}
      [data-edge="nw"],[data-edge="ne"],[data-edge="sw"],[data-edge="se"]{width:${edgeSize}px;height:${edgeSize}px}
      [data-edge="nw"]{left:0;top:0;cursor:nwse-resize}[data-edge="ne"]{right:0;top:0;cursor:nesw-resize}
      [data-edge="sw"]{left:0;bottom:0;cursor:nesw-resize}[data-edge="se"]{right:0;bottom:0;cursor:nwse-resize}
    </style><i data-edge="n"></i><i data-edge="s"></i><i data-edge="e"></i><i data-edge="w"></i>
      <i data-edge="nw"></i><i data-edge="ne"></i><i data-edge="sw"></i><i data-edge="se"></i>`;
    shadow.addEventListener('pointerdown', function(event) {
      const edge = event.target && event.target.dataset ? event.target.dataset.edge : '';
      if (event.button !== 0 || !edge) return;
      event.preventDefault(); event.stopPropagation();
      window.ipc.postMessage(JSON.stringify({ type: 'navigation_resize_start', edge: edge }));
    }, true);
    document.documentElement.appendChild(host);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', installResizeEdges, { once: true });
  } else {
    installResizeEdges();
  }
  addEventListener('load', function() {
    installResizeEdges();
    requestAnimationFrame(() => requestAnimationFrame(() =>
      window.ipc.postMessage('navigation_surface_ready')));
  });
  document.addEventListener('pointerdown', () =>
    window.ipc.postMessage('navigation_surface_interaction'), true);
  document.addEventListener('click', function(event) {
    const anchor = event.target && event.target.closest ? event.target.closest('a[href]') : null;
    if (!anchor || event.defaultPrevented ||
        !/^https?:\/\//i.test(anchor.href)) return;
    event.preventDefault();
    window.ipc.postMessage(JSON.stringify({ type: 'navigation_request', url: anchor.href }));
  }, true);
})();
