// Runs on geometry changes in a worker. Raster size is bounded independently
// of cell count; exact separable distance transforms avoid per-pixel cell loops.
function buildProcessingField(width, height, cells, fixedPadding) {
  const scale = Math.max(1, Math.max(width, height) / 768);
  const ow = Math.max(3, Math.ceil(width / scale)), oh = Math.max(3, Math.ceil(height / scale));
  const valid = cells.filter(r => r.length === 4 && r.every(Number.isFinite) && r[2] > 0 && r[3] > 0
    && r[0] < width && r[1] < height && r[0] + r[2] > 0 && r[1] + r[3] > 0);
  if (!valid.length) return null;
  const thickness = valid.map(r => Math.min(r[2], r[3])).sort((a, b) => a - b);
  const padding = fixedPadding ?? Math.max(18, Math.min(36, thickness[thickness.length >> 1] * 0.8));
  const rounding = Math.max(24, padding);
  // The exterior must exist even when text touches the capture boundary. An
  // edge-clamped distance transform mistakes the crop edge for endless interior.
  const border = Math.ceil((padding + rounding * 3) / scale) + 2;
  const w = ow + border * 2, h = oh + border * 2;
  const stride = w + 1, differences = new Int32Array(stride * (h + 1));
  for (const r of valid) {
    const x0 = border + Math.max(0, Math.min(ow, Math.floor(r[0] / scale)));
    const y0 = border + Math.max(0, Math.min(oh, Math.floor(r[1] / scale)));
    const x1 = border + Math.max(x0 - border, Math.min(ow, Math.ceil((r[0] + r[2]) / scale)));
    const y1 = border + Math.max(y0 - border, Math.min(oh, Math.ceil((r[1] + r[3]) / scale)));
    differences[y0 * stride + x0]++; differences[y0 * stride + x1]--;
    differences[y1 * stride + x0]--; differences[y1 * stride + x1]++;
  }
  const mask = new Uint8Array(w * h);
  for (let y = 0; y < h; y++) for (let x = 0, row = 0; x < w; x++) {
    const index = y * stride + x;
    row += differences[index];
    if (y) differences[index] = row + differences[index - stride];
    else differences[index] = row;
    mask[y * w + x] = Number(differences[index] > 0);
  }
  if (!mask.some(Boolean)) return null;
  function distanceTo(value) {
    const output = new Float32Array(w * h), length = Math.max(w, h);
    const f = new Float64Array(length), d = new Float64Array(length);
    const vertices = new Int32Array(length), limits = new Float64Array(length + 1);
    function transform(n) {
      let k = 0; vertices[0] = 0; limits[0] = -Infinity; limits[1] = Infinity;
      for (let q = 1; q < n; q++) {
        let v = vertices[k], s = ((f[q] + q * q) - (f[v] + v * v)) / (2 * (q - v));
        while (s <= limits[k]) { v = vertices[--k]; s = ((f[q] + q * q) - (f[v] + v * v)) / (2 * (q - v)); }
        vertices[++k] = q; limits[k] = s; limits[k + 1] = Infinity;
      }
      k = 0;
      for (let q = 0; q < n; q++) {
        while (limits[k + 1] < q) k++;
        const delta = q - vertices[k]; d[q] = delta * delta + f[vertices[k]];
      }
    }
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) f[x] = mask[y * w + x] === value ? 0 : 1e12;
      transform(w); output.set(d.subarray(0, w), y * w);
    }
    for (let x = 0; x < w; x++) {
      for (let y = 0; y < h; y++) f[y] = output[y * w + x];
      transform(h);
      for (let y = 0; y < h; y++) output[y * w + x] = d[y];
    }
    return output;
  }
  // Generous padding joins nearby lines. Closing rounds inward notches as well
  // as outward corners, without adding long bridges between distant islands.
  const outer = distanceTo(1), dilation = (padding + rounding) / scale;
  for (let i = 0; i < mask.length; i++) mask[i] = Number(outer[i] <= dilation * dilation);
  // Fill enclosed holes: only the outer silhouette should glow.
  const reached = new Uint8Array(mask.length), queue = new Int32Array(mask.length);
  let head = 0, tail = 0;
  function visit(index) { if (!mask[index] && !reached[index]) { reached[index] = 1; queue[tail++] = index; } }
  for (let x = 0; x < w; x++) { visit(x); visit((h - 1) * w + x); }
  for (let y = 0; y < h; y++) { visit(y * w); visit(y * w + w - 1); }
  while (head < tail) {
    const i = queue[head++], x = i % w, y = Math.floor(i / w);
    if (x) visit(i - 1); if (x + 1 < w) visit(i + 1);
    if (y) visit(i - w); if (y + 1 < h) visit(i + w);
  }
  for (let i = 0; i < mask.length; i++) if (!reached[i]) mask[i] = 1;
  const outside = distanceTo(1), inside = distanceTo(0);
  // Smooth the field itself, not just its zero contour. This also rounds the
  // distance ridges traversed by the rectangle during the in-between frames.
  const signed = new Float32Array(mask.length), softened = new Float32Array(mask.length);
  const step = Math.max(1, Math.round(rounding / (2 * scale)));
  for (let i = 0; i < mask.length; i++) signed[i] = (Math.sqrt(outside[i]) - Math.sqrt(inside[i])) * scale + rounding;
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    const at = dx => signed[y * w + Math.max(0, Math.min(w - 1, x + dx))];
    softened[y * w + x] = (at(-step * 2) + 4 * at(-step) + 6 * at(0) + 4 * at(step) + at(step * 2)) / 16;
  }
  const range = Math.hypot(width, height), pixels = new Uint8Array(ow * oh * 4);
  let left = ow, top = oh, right = 0, bottom = 0;
  for (let i = 0; i < ow * oh; i++) {
    const ox = i % ow, oy = Math.floor(i / ow), x = ox + border, y = oy + border;
    const at = dy => softened[Math.max(0, Math.min(h - 1, y + dy)) * w + x];
    const raw = (at(-step * 2) + 4 * at(-step) + 6 * at(0) + 4 * at(step) + at(step * 2)) / 16;
    const radius = Math.min(28, width / 2, height / 2);
    const qx = Math.abs((ox + .5) * width / ow - width / 2) - (width / 2 - radius - 1);
    const qy = Math.abs((oy + .5) * height / oh - height / 2) - (height / 2 - radius - 1);
    const clip = Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - radius;
    // A rounded intersection has continuous tangents where the silhouette meets
    // the selection edge; a hard max() creates a new corner at every crossing.
    const join = Math.max(0, 24 - Math.abs(raw - clip));
    const distance = Math.max(raw, clip) + join * join / 96;
    if (distance <= 0) { left = Math.min(left, ox); top = Math.min(top, oy); right = Math.max(right, ox + 1); bottom = Math.max(bottom, oy + 1); }
    const encoded = Math.round((Math.max(-1, Math.min(1, distance / range)) + 1) * 32767.5);
    pixels[i * 4] = encoded >> 8; pixels[i * 4 + 1] = encoded & 255; pixels[i * 4 + 3] = 255;
  }
  const bounds = [Math.max(0, left - 1) * width / ow, Math.max(0, top - 1) * height / oh,
    Math.min(ow, right + 1) * width / ow, Math.min(oh, bottom + 1) * height / oh];
  return { width: ow, height: oh, range, padding, bounds, pixels };
}
window.__SGT_BUILD_PROCESSING_FIELD__ = buildProcessingField;
