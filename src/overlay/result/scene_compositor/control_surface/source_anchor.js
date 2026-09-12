// One outer envelope owns the group's controls. Gaps between cells are interior,
// not toolbar slots; geometry is independent of text size and cursor position.
window.__SGT_SOURCE_GEOMETRY__ = function(members, scale) {
  const regions = [];
  const right = r => r.x + r.w, bottom = r => r.y + r.h;
  function add(r) {
    if ([r.x, r.y, r.w, r.h].every(Number.isFinite) && r.w > 0 && r.h > 0) regions.push(r);
  }
  for (const model of members) {
    if (!model?.sourceReplacement || !model.rect) continue;
    const rect = model.rect;
    if (!model.sourceRegions?.length) {
      add({ x: rect.x / scale, y: rect.y / scale, w: rect.width / scale, h: rect.height / scale });
      continue;
    }
    const sx = rect.width / Math.max(1, model.sourceSize?.[0] || rect.width) / scale;
    const sy = rect.height / Math.max(1, model.sourceSize?.[1] || rect.height) / scale;
    for (const line of model.sourceRegions) {
      for (const piece of line.footprint?.length ? line.footprint : [[0, 0, line.width, line.height]]) {
        add({ x: rect.x / scale + (line.x + piece[0]) * sx,
          y: rect.y / scale + (line.y + piece[1]) * sy, w: piece[2] * sx, h: piece[3] * sy });
      }
    }
  }
  if (!regions.length) return null;
  const bounds = { x: Infinity, y: Infinity, w: 0, h: 0 };
  let endX = -Infinity, endY = -Infinity, anchor = regions[0];
  for (const r of regions) {
    bounds.x = Math.min(bounds.x, r.x); bounds.y = Math.min(bounds.y, r.y);
    endX = Math.max(endX, right(r)); endY = Math.max(endY, bottom(r));
    const score = right(r) + bottom(r), current = right(anchor) + bottom(anchor);
    if (score > current || (score === current && (bottom(r) > bottom(anchor)
        || (bottom(r) === bottom(anchor) && r.x > anchor.x)))) anchor = r;
  }
  bounds.w = endX - bounds.x; bounds.h = endY - bounds.y;
  const points = regions.flatMap(r => [[r.x, r.y], [right(r), r.y],
    [right(r), bottom(r)], [r.x, bottom(r)]]).sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const cross = (a, b, c) => (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
  function half(items) {
    const result = [];
    for (const point of items) {
      while (result.length > 1 && cross(result.at(-2), result.at(-1), point) <= 0) result.pop();
      result.push(point);
    }
    result.pop();
    return result;
  }
  const outline = [...half(points), ...half(points.slice().reverse())];
  const planes = outline.map((a, index) => {
    const b = outline[(index + 1) % outline.length], length = Math.hypot(b[0] - a[0], b[1] - a[1]);
    const x = (b[1] - a[1]) / length, y = (a[0] - b[0]) / length;
    return { x, y, limit: x * a[0] + y * a[1] };
  });
  // Keep precise hover discovery inexpensive without making the empty envelope
  // clickable or a hover target. This index contains only actual footprints.
  function build(items) {
    let loX = Infinity, loY = Infinity, hiX = -Infinity, hiY = -Infinity;
    for (const r of items) {
      loX = Math.min(loX, r.x); loY = Math.min(loY, r.y);
      hiX = Math.max(hiX, right(r)); hiY = Math.max(hiY, bottom(r));
    }
    const box = { x: loX, y: loY, w: hiX - loX, h: hiY - loY };
    if (items.length === 1) return { box, item: items[0] };
    const axis = box.w >= box.h ? 'x' : 'y';
    const sorted = items.slice().sort((a, b) => a[axis] - b[axis]);
    const middle = sorted.length >> 1;
    return { box, children: [build(sorted.slice(0, middle)), build(sorted.slice(middle))] };
  }
  return { anchor, obstacles: regions, bounds, outline, planes, proximityIndex: build(regions), source: true };
};

window.__SGT_SOURCE_NEAR__ = function(geometry, x, y, radius) {
  if (!geometry?.source) return false;
  const near = r => {
    const dx = Math.max(r.x - x, 0, x - r.x - r.w);
    const dy = Math.max(r.y - y, 0, y - r.y - r.h);
    return dx * dx + dy * dy <= radius * radius;
  };
  if (geometry.bounds && !near(geometry.bounds)) return false;
  if (geometry.proximityIndex) {
    const stack = [geometry.proximityIndex];
    while (stack.length) {
      const node = stack.pop();
      if (!near(node.box)) continue;
      if (node.item) return true;
      stack.push(...node.children);
    }
    return false;
  }
  return geometry.obstacles.some(near);
};
