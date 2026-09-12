// Visual siblings of the real controls: one layout, no preview hit targets.
window.__SGT_PROCESSING_CONTROLS__ = (function() {
  const previews = new Map();
  function publish(id, rects) {
    window.ipc.postMessage(JSON.stringify({ action: 'processing_control_regions', id, rects }));
  }
  function remove(id) {
    const preview = previews.get(id);
    if (!preview) return;
    previews.delete(id); publish(id, []);
    window.__SGT_PROCESSING_CONTOURS__?.controlCutouts(id, []);
  }
  return {
    layout(group, effect, work) {
      const id = effect.id, scale = devicePixelRatio || 1;
      let preview = previews.get(id);
      if (!preview) {
        preview = { key: '', startedAt: performance.now(), hidden: false }; previews.set(id, preview);
      }
      // Only layout invalidations read geometry, never the animation clock.
      const boxes = [...group.querySelectorAll(':scope > .btn:not(.hidden)')].map(button => {
        const r = button.getBoundingClientRect();
        return { x: r.x, y: r.y, w: r.width, h: r.height,
          radius: parseFloat(getComputedStyle(button).borderTopLeftRadius) || 6 };
      }).filter(r => r.w > 0 && r.h > 0);
      const key = JSON.stringify([boxes, work, scale]);
      if (preview.key === key) return;
      preview.key = key;
      window.__SGT_PROCESSING_CONTOURS__.controlCutouts(id, boxes, work);
      publish(id, boxes.map(r => ({ x: Math.floor((r.x-5)*scale), y: Math.floor((r.y-5)*scale),
        width: Math.ceil((r.w+10)*scale), height: Math.ceil((r.h+10)*scale) })));
      window.__SGT_PROCESSING_CONTOURS__?.wake();
    },
    retain(ids) { for (const id of previews.keys()) if (!ids.has(id)) remove(id); },
    opacity(id, now, reduced) {
      const preview = previews.get(id);
      if (!preview || preview.hidden) return 0;
      const t = preview && !reduced ? Math.max(0, Math.min(1, (now-preview.startedAt)/160)) : 1;
      return t*t*(3-2*t);
    },
    hide(hidden) { for (const preview of previews.values()) preview.hidden = hidden; window.__SGT_PROCESSING_CONTOURS__?.wake(); },
    remove
  };
})();
