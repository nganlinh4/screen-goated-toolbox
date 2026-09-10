// Browser shaping owns the paragraph, including bidi, clusters and line breaks.
// Floats exclude the complement of the source footprint; clipping is a final
// paint guard, never the overflow-fitting strategy.
window.__SGT_FIT_FOOTPRINT__ = function(item) {
  const box = item.box, text = item.text;
  const width = box.clientWidth, height = box.clientHeight;
  const regions = item.footprint.map(function(r) {
    const x = Math.max(0, r[0]), y = Math.max(0, r[1]);
    return [x, y, Math.min(width, r[0] + r[2]) - x,
      Math.min(height, r[1] + r[3]) - y];
  }).filter(r => r[2] > 0 && r[3] > 0);
  if (!regions.length) return false;
  if (regions.length === 1 && regions[0][0] === 0 && regions[0][1] === 0
      && regions[0][2] === width && regions[0][3] === height) return false;
  box.style.clipPath = 'path("' + regions.map(r =>
    'M' + r[0] + ' ' + r[1] + 'h' + r[2] + 'v' + r[3] + 'h-' + r[2] + 'Z'
  ).join(' ') + '")';
  box.style.overflow = 'hidden';

  // A disconnected row uses its widest connected interval, not the blank gap
  // between intervals. The complete paragraph remains a single selectable run.
  // Reserve horizontal ink overhang beyond advance bounds. Vertical font
  // metrics already include ascent/descent; do not subtract that space twice.
  const flowRegions = regions.map(r => [r[0] + 1, r[1], r[2] - 2, r[3]])
    .filter(r => r[2] > 0 && r[3] > 0);
  const edges = [...new Set([0, height, ...flowRegions.flatMap(r => [r[1], r[1] + r[3]])])]
    .sort((a, b) => a - b);
  const bands = edges.slice(0, -1).map(function(y, i) {
    const bottom = edges[i + 1];
    const intervals = flowRegions.filter(r => r[1] <= y && r[1] + r[3] >= bottom)
      .map(r => [r[0], r[0] + r[2]]).sort((a, b) => a[0] - b[0]);
    const merged = [];
    for (const interval of intervals) {
      const last = merged[merged.length - 1];
      if (last && interval[0] <= last[1]) last[1] = Math.max(last[1], interval[1]);
      else merged.push(interval);
    }
    const span = merged.sort((a, b) => (b[1] - b[0]) - (a[1] - a[0]))[0];
    return { y, bottom, left: span ? span[0] : width, right: span ? span[1] : width };
  });

  function fallback() {
    for (const child of [...box.children]) if (child !== text) child.remove();
    const r = regions.reduce((a, b) => a[2] * a[3] >= b[2] * b[3] ? a : b);
    const holder = document.createElement('span');
    holder.style.cssText = 'position:absolute;display:flex;align-items:center;justify-content:center;overflow:hidden;';
    holder.style.left = r[0] + 'px'; holder.style.top = r[1] + 'px';
    holder.style.width = r[2] + 'px'; holder.style.height = r[3] + 'px';
    text.style.display = 'block'; text.style.position = '';
    text.style.fontSize = ''; text.style.lineHeight = '';
    box.appendChild(holder); holder.appendChild(text);
    item.box = holder;
    return false;
  }
  // CSS floats are a horizontal flow primitive. Vertical paragraphs retain
  // native vertical shaping in a contained rectangular source region.
  if (item.vertical || regions.length === 1) return fallback();
  const occupied = bands.filter(b => b.right > b.left);
  const anchor = Math.max(...occupied.map(b => b.left));
  if (!occupied.length || anchor > Math.min(...occupied.map(b => b.right))) return fallback();
  for (const band of bands) if (band.left === band.right) band.left = band.right = anchor;
  box.style.display = 'block'; box.style.textAlign = 'start';
  box.style.fontWeight = '400'; box.style.fontStretch = '100%';
  box.style.fontOpticalSizing = 'none';
  box.style.fontVariationSettings = "'slnt' 0, 'ROND' 100";
  box.setAttribute('dir', 'auto');
  text.style.cssText = 'display:inline;margin:0;padding:0;background:transparent;white-space:normal;overflow-wrap:normal;word-break:normal;font:inherit;';
  function exclusion(side) {
    const node = document.createElement('span');
    const offset = side === 'left' ? 0 : anchor;
    const floatWidth = side === 'left' ? anchor : width - anchor;
    const boundary = bands.flatMap(b => [[b[side] - offset, b.y], [b[side] - offset, b.bottom]]);
    const outside = side === 'left' ? 0 : floatWidth;
    const points = [[outside, 0], ...boundary, [outside, height]];
    node.style.cssText = 'pointer-events:none;float:' + side + ';width:' + floatWidth + 'px;height:100%;';
    node.style.shapeOutside = 'polygon(' + points.map(p => p[0] + 'px ' + p[1] + 'px').join(',') + ')';
    box.insertBefore(node, text);
  }
  exclusion('left'); exclusion('right');
  const origin = box.getBoundingClientRect();
  const range = document.createRange(); range.selectNodeContents(text);
  function fits() {
    return [...range.getClientRects()].every(function(rect) {
      if (!rect.width || !rect.height) return true;
      const left = rect.left - origin.left, right = rect.right - origin.left;
      const top = rect.top - origin.top, bottom = rect.bottom - origin.top;
      if (top < -0.01 || bottom > height + 0.01) return false;
      return bands.filter(b => b.bottom > top + 0.01 && b.y < bottom - 0.01)
        .every(b => left >= b.left - 0.01 && right <= b.right + 0.01);
    });
  }
  const area = bands.reduce((sum, b) => sum + (b.right - b.left) * (b.bottom - b.y), 0);
  const sampleSize = Math.max(1, Number(item.preferredFontSize) || 14);
  box.style.fontSize = sampleSize + 'px'; box.style.lineHeight = 'normal';
  text.style.whiteSpace = 'nowrap';
  const unwrapped = range.getBoundingClientRect();
  const emHeight = Math.max(1, unwrapped.height / sampleSize);
  const ceiling = Math.min(height / emHeight, sampleSize
    * Math.sqrt(area / Math.max(1, unwrapped.width * unwrapped.height)) * 1.2);
  text.style.whiteSpace = 'normal';
  function applySize(size) {
    box.style.fontSize = size + 'px';
    // Use the shaped font's height rather than adding percentage leading on
    // top of the spacing already supplied by the source-line footprint.
    box.style.lineHeight = (Math.ceil(size * emHeight) + 1) + 'px';
  }
  let best = 0, upper = ceiling;
  // Establish a fitting candidate, then spend a fixed budget growing it.
  // Keep only measured fits: float line placement can have discontinuities.
  for (const ratio of [1, 0.82, 0.64, 0.46]) {
    const size = ceiling * ratio;
    applySize(size);
    if (fits()) { best = size; break; }
    upper = size;
  }
  if (best > 0) {
    for (let probe = 0; probe < 5 && upper - best > 0.05; probe++) {
      const size = (best + upper) / 2;
      applySize(size);
      if (fits()) best = size;
      else upper = size;
    }
    let stretch = 100;
    // Trade width for genuinely larger shaped glyphs, not a bitmap transform.
    // Retain normal width unless condensation buys a visible height increase.
    box.style.fontStretch = '60%';
    applySize(sampleSize);
    text.style.whiteSpace = 'nowrap';
    const condensed = range.getBoundingClientRect();
    text.style.whiteSpace = 'normal';
    // Fallback fonts without a width axis cannot benefit from these probes.
    if (condensed.width < unwrapped.width - 0.01) {
      let low = best, high = Math.min(height / emHeight, sampleSize
        * Math.sqrt(area / Math.max(1, condensed.width * condensed.height)) * 1.2);
      let measured = 0;
      for (let probe = 0; probe < 5; probe++) {
        const size = (low + high) / 2;
        applySize(size);
        if (fits()) { measured = size; low = size; }
        else high = size;
      }
      if (measured >= best * 1.06) {
        best = measured; stretch = 60;
        applySize(best);
        // Recover the widest glyphs that can retain the selected height.
        for (const candidate of [85, 70]) {
          box.style.fontStretch = candidate + '%';
          if (fits()) { stretch = candidate; break; }
        }
      }
    }
    box.style.fontStretch = stretch + '%';
    applySize(best);
    item.stretch = stretch;
    item.fontSize = best; item.visualScale = 1;
    box.dataset.footprintFit = 'flow';
    return true;
  }
  box.dataset.footprintFit = 'rectangle';
  return fallback();
};
