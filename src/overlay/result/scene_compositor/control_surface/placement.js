// Pure geometry shared by ordinary results and source-replacement groups.
window.__SGT_CONTROL_PLACEMENT__ = (function() {
  const gap = 4;
  const right = r => r.x + r.w;
  const bottom = r => r.y + r.h;
  const valid = r => r && [r.x, r.y, r.w, r.h].every(Number.isFinite) && r.w > 0 && r.h > 0;
  const intersection = (a, b) => Math.max(0, Math.min(right(a), right(b)) - Math.max(a.x, b.x))
    * Math.max(0, Math.min(bottom(a), bottom(b)) - Math.max(a.y, b.y));
  const distance = (p, r) => Math.hypot(Math.max(r.x - p.x, 0, p.x - right(r)),
    Math.max(r.y - p.y, 0, p.y - bottom(r)));

  function sourceGeometry(members, scale) {
    return window.__SGT_SOURCE_GEOMETRY__(members, scale);
  }

  function monitorAt(anchor, monitors) {
    // Select the last physical pixel within the anchor, including at a monitor seam.
    const pixel = 1 / (window.devicePixelRatio || 1);
    const point = { x: anchor.x + Math.max(anchor.w, pixel) - pixel,
      y: anchor.y + Math.max(anchor.h, pixel) - pixel };
    let chosen = null, nearest = Infinity;
    for (const monitor of monitors) {
      if (!valid(monitor.bounds) || !valid(monitor.work)) continue;
      const d = distance(point, monitor.bounds);
      if (d < nearest) { chosen = monitor; nearest = d; }
      if (point.x >= monitor.bounds.x && point.x < right(monitor.bounds)
          && point.y >= monitor.bounds.y && point.y < bottom(monitor.bounds)) return monitor;
    }
    return chosen;
  }

  function workArea(anchor, monitors, fallback) {
    return monitorAt(anchor, monitors)?.work || fallback;
  }

  function monitorScale(anchor, monitors) {
    const dpi = Number(monitorAt(anchor, monitors)?.dpi);
    const deviceScale = window.devicePixelRatio || 1;
    return Number.isFinite(dpi) && dpi > 0 ? dpi / (96 * deviceScale) : 1 / deviceScale;
  }

  function preferred(anchor, size, direction) {
    if (direction === 'bottom') return { x: right(anchor) - size.w, y: bottom(anchor) + gap };
    if (direction === 'top') return { x: anchor.x + (anchor.w - size.w) / 2,
      y: anchor.y - size.h - gap };
    if (direction === 'right') return { x: right(anchor) + gap,
      y: anchor.y + (anchor.h - size.h) / 2 };
    return { x: anchor.x - size.w - gap,
      y: anchor.y + (anchor.h - size.h) / 2 };
  }

  function place(geometry, sizes, work, previous, locked) {
    if (geometry.source) return window.__SGT_SOURCE_DOCK__(geometry, sizes, work, previous, locked);
    const { anchor } = geometry;
    const obstacles = geometry.obstacles || [anchor];
    const candidates = [];
    for (const [order, direction] of ['bottom', 'right', 'left', 'top'].entries()) {
      const vertical = direction === 'left' || direction === 'right';
      if (locked && previous && direction !== previous.direction) continue;
      const natural = sizes[vertical ? 'vertical' : 'horizontal'];
      if (!natural || !valid({ x: 0, y: 0, ...natural })) continue;
      const size = { w: Math.min(natural.w, work.w), h: Math.min(natural.h, work.h) };
      const wanted = preferred(anchor, size, direction);
      const base = { x: Math.max(work.x, Math.min(wanted.x, right(work) - size.w)),
        y: Math.max(work.y, Math.min(wanted.y, bottom(work) - size.h)), ...size };
      const overlap = obstacles.reduce((sum, r) => sum + intersection(base, r), 0);
      const overflow = natural.w * natural.h - size.w * size.h;
      const preference = order * 10000 + Math.hypot(base.x - wanted.x, base.y - wanted.y);
      candidates.push({ ...base, direction, vertical, overflow, overlap, preference });
    }
    candidates.sort((a, b) => a.overflow - b.overflow || a.overlap - b.overlap || a.preference - b.preference);
    const best = candidates[0];
    if (!best) return null;
    // Minor geometry/measurement noise must not flip the toolbar to another side.
    const stable = previous && candidates.find(c => c.direction === previous.direction
      && c.overflow <= best.overflow && c.overlap <= best.overlap
      && (locked || c.preference <= best.preference + 12));
    return { ...(stable || best), work };
  }

  return { sourceGeometry, workArea, monitorScale, place };
})();
