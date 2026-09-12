// Dock outside the whole content envelope, never in an inter-cell gap. Each
// outline edge defines one exterior half-plane; project onto its screen-clipped
// boundary instead of searching pixels or guessing which cluster matters most.
window.__SGT_SOURCE_DOCK__ = function(geometry, sizes, work, previous, locked) {
  const gap = 4, epsilon = 0.01, anchor = geometry.anchor;
  const right = r => r.x + r.w, bottom = r => r.y + r.h;
  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
  const valid = s => s && Number.isFinite(s.w) && Number.isFinite(s.h) && s.w > 0 && s.h > 0;
  const tip = { x: clamp(right(anchor), work.x, right(work)), y: clamp(bottom(anchor), work.y, bottom(work)) };
  const outside = r => geometry.planes.some(p => p.x * r.x + p.y * r.y
    + Math.min(0, p.x * r.w) + Math.min(0, p.y * r.h) >= p.limit + gap - epsilon);
  const inside = r => r.x >= work.x - epsilon && r.y >= work.y - epsilon
    && right(r) <= right(work) + epsilon && bottom(r) <= bottom(work) + epsilon;
  function dimensions(natural) {
    return { w: Math.min(Math.max(natural.w, natural.reserveW || 0), work.w),
      h: Math.min(Math.max(natural.h, natural.reserveH || 0), work.h) };
  }
  function packed(reserved, natural, vertical, direction, fallback = false) {
    const w = Math.min(natural.w, reserved.w), h = Math.min(natural.h, reserved.h);
    return { x: right(reserved) - w, y: vertical ? bottom(reserved) - h : reserved.y,
      w, h, reserved, vertical, direction, fallback, work, anchor: { ...anchor } };
  }
  // Expansion was reserved before reveal. All intermediate animation frames
  // stay in that exterior slot with a fixed trailing edge and no enlarged hitbox.
  if (locked && previous?.reserved) {
    const natural = sizes[previous.vertical ? 'vertical' : 'horizontal'];
    if (valid(natural)) {
      const size = dimensions(natural), old = previous.reserved;
      if (size.w <= old.w + epsilon && size.h <= old.h + epsilon && inside(old)
          && (outside(old) || previous.fallback)) {
        return packed(old, natural, previous.vertical, previous.direction, previous.fallback);
      }
    }
  }
  const candidates = [];
  for (const vertical of [false, true]) {
    const natural = sizes[vertical ? 'vertical' : 'horizontal'];
    if (!valid(natural)) continue;
    const size = dimensions(natural);
    const wanted = vertical ? { x: tip.x + gap, y: tip.y - size.h }
      : { x: tip.x - size.w, y: tip.y + gap };
    const range = { x: work.x, y: work.y, w: work.w - size.w, h: work.h - size.h };
    const corners = [{ x: range.x, y: range.y }, { x: right(range), y: range.y },
      { x: right(range), y: bottom(range) }, { x: range.x, y: bottom(range) }];
    const clamped = { x: clamp(wanted.x, range.x, right(range)), y: clamp(wanted.y, range.y, bottom(range)) };
    for (const plane of geometry.planes) {
      const limit = plane.limit + gap - Math.min(0, plane.x * size.w) - Math.min(0, plane.y * size.h);
      const value = point => plane.x * point.x + plane.y * point.y - limit;
      let point = clamped;
      if (value(point) < -epsilon) {
        const options = corners.filter(p => value(p) >= -epsilon);
        const deficit = -value(wanted);
        const projected = { x: wanted.x + plane.x * deficit, y: wanted.y + plane.y * deficit };
        if (projected.x >= range.x && projected.x <= right(range)
            && projected.y >= range.y && projected.y <= bottom(range)) options.push(projected);
        for (let index = 0; index < corners.length; index++) {
          const a = corners[index], b = corners[(index + 1) % corners.length];
          const va = value(a), vb = value(b);
          if (va * vb < 0) {
            const t = va / (va - vb);
            options.push({ x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t });
          }
        }
        options.sort((a, b) => Math.hypot(a.x - wanted.x, a.y - wanted.y)
          - Math.hypot(b.x - wanted.x, b.y - wanted.y));
        point = options[0];
      }
      if (!point) continue;
      const direction = Math.abs(plane.y) >= Math.abs(plane.x)
        ? (plane.y > 0 ? 'bottom' : 'top') : (plane.x > 0 ? 'right' : 'left');
      const result = packed({ ...point, ...size }, natural, vertical, direction);
      // A nearby row beats a column, but never travel to the opposite screen
      // edge just to retain a horizontal orientation.
      const preference = Math.hypot(point.x - wanted.x, point.y - wanted.y)
        + (vertical ? Math.max(64, (sizes.horizontal?.h || 32) * 3) : 0);
      candidates.push({ ...result, preference });
    }
  }
  candidates.sort((a, b) => a.preference - b.preference || Number(a.vertical) - Number(b.vertical));
  if (candidates.length) return candidates[0];

  // A full-screen content envelope may leave no exterior slot at all. In that
  // case use the screen perimeter, not an apparently free hole among the cells.
  const fallback = [];
  for (const direction of ['bottom', 'right', 'top', 'left']) {
    const vertical = direction === 'right' || direction === 'left';
    const natural = sizes[vertical ? 'vertical' : 'horizontal'];
    if (!valid(natural)) continue;
    const size = dimensions(natural);
    const reserved = { ...size, x: vertical ? (direction === 'right' ? right(work) - size.w : work.x)
      : clamp(tip.x - size.w, work.x, right(work) - size.w),
    y: vertical ? clamp(tip.y - size.h, work.y, bottom(work) - size.h)
      : (direction === 'bottom' ? bottom(work) - size.h : work.y) };
    const overlap = geometry.obstacles.reduce((sum, r) => sum
      + Math.max(0, Math.min(right(r), right(reserved)) - Math.max(r.x, reserved.x))
      * Math.max(0, Math.min(bottom(r), bottom(reserved)) - Math.max(r.y, reserved.y)), 0);
    const distance = Math.hypot(Math.max(reserved.x - tip.x, 0, tip.x - right(reserved)),
      Math.max(reserved.y - tip.y, 0, tip.y - bottom(reserved)));
    fallback.push({ ...packed(reserved, natural, vertical, direction, true), overlap, distance });
  }
  fallback.sort((a, b) => a.overlap - b.overlap || a.distance - b.distance || Number(a.vertical) - Number(b.vertical));
  return fallback[0] || null;
};
