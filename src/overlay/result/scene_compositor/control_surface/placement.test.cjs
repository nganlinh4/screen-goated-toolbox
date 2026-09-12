const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const vm = require('node:vm');
const scope = vm.createContext({ window: {}, atob, Uint8Array, DataView });
for (const script of ['source_anchor.js', 'source_dock.js', 'placement.js', 'source_geometry.js']) {
  vm.runInContext(readFileSync(join(__dirname, script), 'utf8'), scope);
}
const placement = scope.window.__SGT_CONTROL_PLACEMENT__;
const r = (x, y, w, h) => ({ x, y, w, h });
const normalSizes = { horizontal: { w: 200, h: 32 }, vertical: { w: 40, h: 210 } };
const area = r(0, 0, 1000, 760);
const json = value => JSON.parse(JSON.stringify(value));
function inside(pos, bounds = area) {
  assert.ok(pos.x >= bounds.x && pos.y >= bounds.y, JSON.stringify(pos));
  assert.ok(pos.x + pos.w <= bounds.x + bounds.w + 0.01, JSON.stringify(pos));
  assert.ok(pos.y + pos.h <= bounds.y + bounds.h + 0.01, JSON.stringify(pos));
}
function source(rects) { return placement.sourceGeometry(rects.map((rect, id) => model(id, rect)), 1); }
function model(id, rect, regions) {
  return { id, rect: { x: rect.x, y: rect.y, width: rect.w, height: rect.h },
    sourceReplacement: true, sourceRegions: regions };
}

// Independently clip the actual polygon against the toolbar, rather than
// asserting the solver's own collision score or its chosen half-plane.
function exterior(pos, geometry) {
  assert.equal(pos.fallback, false);
  const rect = pos.reserved || pos;
  let polygon = geometry.outline.map(p => [...p]);
  for (const [axis, limit, sign] of [[0, rect.x, 1], [0, rect.x + rect.w, -1],
    [1, rect.y, 1], [1, rect.y + rect.h, -1]]) {
    const output = [];
    for (let i = 0; i < polygon.length; i++) {
      const a = polygon[i], b = polygon[(i + 1) % polygon.length];
      const va = (a[axis] - limit) * sign, vb = (b[axis] - limit) * sign;
      if (va >= 0) output.push(a);
      if (va * vb < 0) {
        const t = va / (va - vb);
        output.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
      }
    }
    polygon = output;
  }
  const area = Math.abs(polygon.reduce((sum, p, i) => {
    const q = polygon[(i + 1) % polygon.length];
    return sum + p[0] * q[1] - p[1] * q[0];
  }, 0)) / 2;
  assert.ok(area < 0.01, JSON.stringify({ pos, outline: geometry.outline, area }));
}

test('southeast anchor selects real content rather than the empty union corner', () => {
  const geometry = placement.sourceGeometry([model(1, r(700, 100, 100, 40)), model(2, r(100, 650, 160, 40))], 1);
  assert.deepEqual(json(geometry.anchor), r(100, 650, 160, 40));
  assert.notEqual(geometry.anchor.x + geometry.anchor.w, 800);
  const pos = placement.place(geometry, normalSizes, area);
  assert.equal(pos.direction, 'bottom');
  assert.equal(pos.x + pos.w, 260);
  assert.equal(pos.y, 694);
  exterior(pos, geometry);
});

test('footprint holes, scaling, resized cards and negative origins preserve geometry', () => {
  const item = model(1, r(-300, -100, 400, 200), [
    { x: 0, y: 0, width: 200, height: 100, footprint: [[0, 80, 40, 20], [150, 0, 50, 20]] },
  ]);
  item.sourceSize = [200, 100];
  const geometry = placement.sourceGeometry([item], 2);
  assert.deepEqual(json(geometry.anchor), r(0, -50, 50, 20));
  assert.equal(geometry.obstacles.length, 2);
});

test('equally close footprints are deterministic regardless of input order', () => {
  const a = model(1, r(100, 200, 20, 20)), b = model(2, r(200, 100, 20, 20));
  assert.deepEqual(json(placement.sourceGeometry([a, b], 1).anchor), json(placement.sourceGeometry([b, a], 1).anchor));
  assert.equal(placement.sourceGeometry([a, b], 1).anchor.y, 200);
  const c = model(3, r(90, 190, 30, 30));
  assert.deepEqual(json(placement.sourceGeometry([a, c], 1).anchor), json(placement.sourceGeometry([c, a], 1).anchor));
});

test('all outer cells participate regardless of text area', () => {
  const body = model(1, r(200, 200, 500, 300));
  const label = model(2, r(1250, 760, 30, 18));
  const geometry = placement.sourceGeometry([body, label], 1);
  assert.deepEqual(json(geometry.anchor), r(1250, 760, 30, 18));
  exterior(placement.place(geometry, normalSizes, r(0, 0, 1600, 900)), geometry);
});

test('dense neighboring text anchors on the outside edge, not an interior fragment', () => {
  const members = [];
  for (let y = 100; y <= 600; y += 50) for (let x = 100; x <= 800; x += 140) {
    members.push(model(members.length, r(x, y, 100, 20)));
  }
  const geometry = placement.sourceGeometry(members, 1);
  assert.deepEqual(json(geometry.anchor), r(800, 600, 100, 20));
  const pos = placement.place(geometry, normalSizes, r(0, 0, 1600, 900));
  exterior(pos, geometry);
  assert.equal(pos.y, 624);
});

test('disconnected islands share one envelope but empty gaps do not reveal controls', () => {
  const models = [model(1, r(100, 500, 100, 100)), model(2, r(500, 100, 100, 100))];
  for (const geometry of [placement.sourceGeometry(models, 1), placement.sourceGeometry(models.reverse(), 1)]) {
    assert.equal(geometry.anchor.x, 100);
    assert.equal(scope.window.__SGT_SOURCE_NEAR__(geometry, 150, 550, 40), true);
    assert.equal(scope.window.__SGT_SOURCE_NEAR__(geometry, 550, 150, 40), true);
    assert.equal(scope.window.__SGT_SOURCE_NEAR__(geometry, 350, 350, 40), false);
  }
});

test('cached proximity index is exact in text, holes and disconnected islands', () => {
  const members = [];
  for (let i = 0; i < 120; i++) members.push(model(i, r((i * 173) % 1400, (i * 97) % 800, 30, 15)));
  const geometry = placement.sourceGeometry(members, 1);
  for (let i = 0; i < 1000; i++) {
    const x = (i * 127) % 1700, y = (i * 131) % 1000;
    const expected = geometry.obstacles.some(r => {
      const dx = Math.max(r.x - x, 0, x - r.x - r.w), dy = Math.max(r.y - y, 0, y - r.y - r.h);
      return dx * dx + dy * dy <= 1600;
    });
    assert.equal(scope.window.__SGT_SOURCE_NEAR__(geometry, x, y, 40), expected);
  }
});

test('the outer envelope follows restore and movement', () => {
  const cache = scope.window.__SGT_CONTROL_GEOMETRY_CACHE__();
  const first = model(1, r(100, 500, 100, 100)), second = model(2, r(500, 100, 100, 100));
  const models = new Map([['1', first], ['2', second]]);
  const root = { id: 3, controlRect: { x: 0, y: 0, width: 800, height: 800 },
    controls: { groupActions: true, groupIds: [1, 2] } };
  const initial = cache.get(root, models, 1);
  assert.equal(initial.anchor.x, 100);
  first.rect = { ...first.rect, x: 400, y: 450 };
  second.rect = { ...second.rect, x: 800, y: 50 };
  root.controlRect = { ...root.controlRect, x: 300, y: -50 };
  cache.merge(first, {}); cache.merge(second, {}); cache.merge(root, {});
  const moved = cache.get(root, models, 1);
  assert.equal(moved.anchor.x, initial.anchor.x + 300);
  assert.equal(moved.anchor.y, initial.anchor.y - 50);
  cache.clear();
  assert.deepEqual(json(cache.get(root, models, 1).anchor), json(moved.anchor));
});

test('a prominent interior card cannot own the toolbar in a sparse layout', () => {
  const geometry = source([r(100, 100, 400, 200), r(100, 600, 200, 30), r(700, 400, 50, 30)]);
  const pos = placement.place(geometry, normalSizes, area);
  exterior(pos, geometry);
  assert.notEqual(pos.y, 304);
});

test('the toolbar clears the complete sloping outline', () => {
  const anchor = r(1392, 716, 177, 42);
  const geometry = source([r(1362, 657, 115, 40), r(1277, 771, 169, 46),
    r(1265, 451, 124, 66), r(1434, 484, 166, 43), anchor]);
  const work = r(0, 0, 1600, 860);
  const pos = placement.place(geometry, normalSizes, work);
  assert.equal(pos.direction, 'bottom');
  exterior(pos, geometry);
  inside(pos, work);
});

test('the bottom screen edge prefers a familiar row above content', () => {
  const geometry = source([r(650, 810, 90, 40)]);
  const pos = placement.place(geometry, normalSizes, r(0, 0, 1600, 860));
  assert.equal(pos.direction, 'top');
  assert.equal(pos.vertical, false);
  exterior(pos, geometry);
});

test('a screen-filling envelope falls back to the screen perimeter, never an interior hole', () => {
  const geometry = source([r(0, 0, 30, 30), r(970, 0, 30, 30), r(0, 730, 30, 30), r(970, 730, 30, 30)]);
  const pos = placement.place(geometry, normalSizes, area);
  assert.equal(pos.fallback, true);
  assert.ok(pos.x === 0 || pos.y === 0 || pos.x + pos.w === 1000 || pos.y + pos.h === 760);
  inside(pos);
});

test('approach locking preserves the exact trailing edge during expansion', () => {
  const geometry = source([r(200, 200, 100, 40)]);
  const sizes = { ...normalSizes, horizontal: { w: 200, h: 32, reserveW: 260 } };
  const previous = placement.place(geometry, sizes, area);
  const next = placement.place(geometry, { ...sizes, horizontal: { w: 260, h: 32 } }, area, previous, true);
  assert.equal(next.x + next.w, previous.x + previous.w);
  assert.equal(next.y, previous.y);
  assert.equal(next.direction, previous.direction);
  exterior(next, geometry);
});

test('sparse and diagonal layouts keep the full slider expansion outside the envelope', () => {
  const sizes = { horizontal: { w: 200, h: 32, reserveW: 330 }, vertical: { w: 40, h: 210, reserveH: 340 } };
  for (let scene = 0; scene < 120; scene++) {
    const cells = Array.from({ length: 3 + scene % 20 }, (_, index) =>
      r(80 + (scene * 79 + index * 173) % 760, 80 + (scene * 97 + index * 137) % 510, 50, 25));
    const geometry = source(cells), pos = placement.place(geometry, sizes, area);
    inside(pos.reserved);
    exterior(pos, geometry);
    const expanded = placement.place(geometry,
      { horizontal: { w: 330, h: 32 }, vertical: { w: 40, h: 340 } }, area, pos, true);
    inside(expanded);
    exterior(expanded, geometry);
    assert.equal(expanded.x + expanded.w, pos.x + pos.w);
    assert.equal(expanded.direction, pos.direction);
  }
});

test('normal results retain bottom-right preference and adaptive side fallback', () => {
  const anchor = r(300, 200, 400, 220);
  const first = placement.place({ anchor, source: false }, normalSizes, area);
  assert.equal(first.direction, 'bottom');
  assert.equal(first.x, 500);
  assert.equal(first.y, 424);
  const edge = placement.place({ anchor: r(300, 550, 400, 200), source: false }, normalSizes, area);
  assert.equal(edge.direction, 'right');
  assert.equal(edge.overlap, 0);
  inside(edge);
});

test('each screen edge and corner stays inside its work area', () => {
  for (const x of [0, 1, 400, 959, 980]) for (const y of [0, 1, 350, 739, 750]) {
    const pos = placement.place(source([r(x, y, 40, 20)]), normalSizes, area);
    inside(pos);
  }
});

test('nearby content is avoided even when the default row would overlap it', () => {
  const geometry = source([r(100, 620, 230, 65), r(310, 580, 60, 35)]);
  const pos = placement.place(geometry, normalSizes, area);
  exterior(pos, geometry);
  inside(pos);
});

test('interior gaps are forbidden even when there is enough room for the buttons', () => {
  const geometry = source([r(150, 324, 65, 32), r(350, 100, 30, 150), r(300, 280, 40, 40)]);
  const pos = placement.place(geometry, normalSizes, area);
  exterior(pos, geometry);
});

test('crowded and smaller-than-toolbar surfaces have a bounded fallback', () => {
  for (const work of [r(0, 0, 120, 80), r(-40, 20, 500, 300)]) {
    const pos = placement.place(source([work]), normalSizes, work);
    inside(pos, work);
    assert.ok(Number.isFinite(pos.overlap));
  }
});

test('monitor seams, taskbars, unequal monitors and desktop gaps use physical bounds', () => {
  const monitors = [
    { bounds: r(0, 0, 1200, 900), work: r(40, 0, 1160, 860) },
    { bounds: r(1200, 300, 1600, 1000), work: r(1200, 340, 1600, 960) },
  ];
  assert.deepEqual(placement.workArea(r(1150, 700, 50, 40), monitors, area), monitors[0].work);
  assert.deepEqual(placement.workArea(r(1300, 500, 80, 30), monitors, area), monitors[1].work);
  assert.deepEqual(placement.workArea(r(1210, 0, 40, 20), monitors, area), monitors[0].work);
  const anchor = r(1120, 820, 70, 40);
  inside(placement.place(source([anchor]), normalSizes, monitors[0].work), monitors[0].work);
});

test('hover and opacity expansion never change sides; the expanded bar remains on screen', () => {
  const geometry = source([r(40, 720, 70, 20)]);
  const sizes = { horizontal: { w: 200, h: 32, reserveW: 330 }, vertical: { w: 40, h: 210, reserveH: 340 } };
  const previous = placement.place(geometry, sizes, area);
  const expanded = { horizontal: { w: 330, h: 32 }, vertical: { w: 40, h: 340 } };
  const next = placement.place(geometry, expanded, area, previous, true);
  assert.equal(next.direction, previous.direction);
  inside(next);
});

test('geometry cache ignores streamed text, visibility and completion order', () => {
  const cache = scope.window.__SGT_CONTROL_GEOMETRY_CACHE__();
  const first = model(1, r(20, 50, 80, 30));
  const second = model(2, r(300, 200, 80, 30));
  const models = new Map([['1', first], ['2', second]]);
  const root = { id: 3, controls: { groupActions: true, groupIds: [3, 1, 2] } };
  cache.merge(first, { source_replacement: true });
  cache.merge(second, { source_replacement: true });
  const geometry = cache.get(root, models, 1);
  for (let i = 0; i < 300; i++) {
    cache.merge(first, { body: String(i), visible: true, source_replacement: true });
    assert.equal(cache.get(root, models, 1), geometry);
  }
  second.rect = { ...second.rect, x: 400 };
  cache.merge(second, {});
  assert.notEqual(cache.get(root, models, 1), geometry);
  assert.equal(cache.get(root, models, 1).anchor.x, 400);
  models.delete('2'); cache.clear();
  assert.equal(cache.get(root, models, 1).anchor.x, 20);
});

test('restored resized source geometry uses the backdrop size, not the new window size', () => {
  const bytes = Buffer.alloc(24);
  bytes.writeUInt32BE(0x89504e47, 0); bytes.writeUInt32BE(0x49484452, 12);
  bytes.writeUInt32BE(100, 16); bytes.writeUInt32BE(50, 20);
  const cache = scope.window.__SGT_CONTROL_GEOMETRY_CACHE__();
  const card = model(1, r(0, 0, 200, 100), [{ x: 0, y: 0, width: 80, height: 40 }]);
  cache.merge(card, { source_replacement: true, backdrop_data_url: 'data:image/png;base64,' + bytes.toString('base64') });
  assert.deepEqual(json(card.sourceSize), [100, 50]);
  assert.deepEqual(json(placement.sourceGeometry([card], 1).anchor), r(0, 0, 160, 80));
  cache.merge(card, { backdrop_data_url: '' });
  assert.equal(card.sourceSize, undefined);
  assert.deepEqual(json(placement.sourceGeometry([card], 1).anchor), r(0, 0, 80, 40));
});
