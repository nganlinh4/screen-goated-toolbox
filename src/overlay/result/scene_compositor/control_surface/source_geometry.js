// Cache immutable source geometry separately from streaming text and visibility.
window.__SGT_CONTROL_GEOMETRY_CACHE__ = function() {
  const cache = new Map();
  function merge(current, model) {
    const old = current.controlGeometryKey;
    if (model.source_replacement !== undefined) current.sourceReplacement = model.source_replacement === true;
    if (model.source_regions !== undefined) current.sourceRegions = model.source_regions;
    if (model.backdrop_data_url !== undefined && current.sourceReplacement) {
      const header = String(model.backdrop_data_url || '').slice(0, 80);
      if (header !== current.sourceHeader) {
        current.sourceHeader = header;
        current.sourceSize = undefined;
        // Source backdrops are PNGs. Read only their size header, never image pixels.
        const prefix = 'data:image/png;base64,';
        if (header.startsWith(prefix)) {
          try {
            const bytes = Uint8Array.from(atob(header.slice(prefix.length, prefix.length + 32)), c => c.charCodeAt(0));
            const view = new DataView(bytes.buffer);
            if (view.getUint32(0) === 0x89504e47 && view.getUint32(12) === 0x49484452) {
              current.sourceSize = [view.getUint32(16), view.getUint32(20)];
            }
          } catch (_) { /* Non-PNG surfaces retain their declared scene geometry. */ }
        }
      }
    }
    current.controlGeometryKey = JSON.stringify([current.rect, current.controlRect, current.sourceReplacement,
      current.sourceSize, current.sourceRegions]);
    if (old !== current.controlGeometryKey) cache.clear();
  }
  function get(model, models, scale) {
    if (!model.controls?.groupActions) return null;
    const ids = model.controls.groupIds || [];
    const key = JSON.stringify([model.id, ids, scale]);
    if (!cache.has(key)) {
      const members = ids.map(id => models.get(String(id))).filter(Boolean);
      cache.set(key, window.__SGT_CONTROL_PLACEMENT__.sourceGeometry(members, scale));
    }
    return cache.get(key);
  }
  return { merge, get, clear: () => cache.clear() };
};
