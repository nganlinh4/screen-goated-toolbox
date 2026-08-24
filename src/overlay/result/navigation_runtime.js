(function() {
  addEventListener('load', function() {
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
