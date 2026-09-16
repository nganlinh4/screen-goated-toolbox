const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const { join } = require('node:path');
const scope = vm.createContext({ window: {}, Math, Number, Float32Array, Float64Array, Int32Array, Uint8Array });
vm.runInContext(fs.readFileSync(join(__dirname, 'contour_field.js'), 'utf8'), scope);
const runtime = fs.readFileSync(join(__dirname, 'contour_runtime.js'), 'utf8');
vm.runInContext(runtime.split('window.__SGT_PROCESSING_CONTOURS__')[0], scope);
const build = scope.window.__SGT_BUILD_PROCESSING_FIELD__;
const timeline = scope.window.__SGT_PROCESSING_TIMELINE__;
function distance(field, width, height, x, y) {
  const i = (Math.min(field.height - 1, Math.floor(y / height * field.height)) * field.width
    + Math.min(field.width - 1, Math.floor(x / width * field.width))) * 4;
  return ((field.pixels[i] * 256 + field.pixels[i + 1]) / 65535 * 2 - 1) * field.range;
}
test('padding merges neighboring cells while retaining deep notches', () => {
  const field = build(440, 340, [[80,50,180,25],[80,95,140,25],[95,140,60,20],[205,170,45,35],[80,245,260,25]]);
  for (const [x,y] of [[90,60],[140,90],[110,150],[220,180],[120,255]]) assert.ok(distance(field,440,340,x,y) < 0);
  assert.ok(distance(field,440,340,300,160) > 15, 'a wide gap must stay a notch');
  assert.ok(distance(field,440,340,70,60) < 0, 'contour has breathing room around text');
});
test('distant islands do not get long bridges and enclosed holes are filled', () => {
  const separated = build(800, 400, [[40,40,100,30],[650,300,80,30]]);
  assert.ok(distance(separated,800,400,400,200) > 100);
  const ring = build(400,400,[[80,80,240,20],[80,300,240,20],[80,80,20,240],[300,80,20,240]]);
  assert.ok(distance(ring,400,400,200,200) < 0);
});
test('empty, clipped and dense geometry stays bounded without per-pixel cell work', () => {
  assert.equal(build(100,100,[]), null);
  assert.equal(build(100,100,[[200,200,10,10],[0,0,0,5]]), null);
  const cells = Array.from({length:2000},(_,i) => [(i%50)*70,Math.floor(i/50)*50,50,20]);
  const field = build(3840,2160,cells);
  assert.ok(field.width <= 768 && field.height <= 768);
  assert.equal(field.pixels.length,field.width*field.height*4);
  for (const [x,y] of [[2,2],[352,252],[700,500]]) assert.ok(Number.isFinite(distance(field,3840,2160,x,y)));
});
test('morph is monotonic, keeps the color clock and never gates result delivery', () => {
  const entry = { startedAt:0, morphAt:200, closedAt:null };
  let last=0;
  for(let now=200;now<=1100;now+=10) {
    const state=timeline(entry,now,false);
    assert.ok(state.morph>=last && state.morph<=1);last=state.morph;
    assert.equal(state.phase,now/1800);assert.equal(state.opacity,1);
  }
  assert.equal(last,1);
  assert.ok(timeline(entry,500,false).morph < .6, 'initial travel must not rush through its silhouette');
  assert.equal(timeline({...entry,closedAt:300,finish:true},300,false).opacity,1);
  assert.equal(timeline({...entry,closedAt:300,finish:true},660,false).done,true);
  assert.equal(timeline({...entry,closedAt:300,finish:false},520,false).done,true);
  assert.equal(timeline(entry,201,true).morph,1);
});

test('completion never waits for unfinished geometry; empty work retreats before host acknowledgement', () => {
  const entry = { startedAt:0, morphAt:null, pendingAt:100, closedAt:150, finish:true };
  assert.ok(timeline(entry,400,false).opacity < .3);
  assert.equal(timeline(entry,510,false).done,true);
  assert.equal(timeline({...entry,finish:false},370,false).done,true);
  const ready = {...entry,pendingAt:null,morphAt:180};
  assert.equal(timeline(ready,510,false).done,true);
  const empty = {...ready,emptyAt:200,closedAt:null};
  assert.equal(timeline(empty,560,false).opacity,0);
  assert.equal(timeline(empty,560,false).done,false);
  assert.equal(timeline({...empty,closedAt:570},570,false).done,true);
});

test('edge travel settles without overshoot or a velocity snap; softness clears at rest', () => {
  const entry = {startedAt:0,morphAt:0,closedAt:null};
  let previous = timeline(entry,0,false);
  for(let time=1;time<=900;time++) {
    const state=timeline(entry,time,false);
    for(const key of ['contract','morph']) assert.ok(state[key]>=previous[key]-1e-12 && state[key]<=1+1e-12);
    assert.ok(state.contract>=state.morph-1e-12);
    previous=state;
  }
  for(const key of ['contract','morph']) assert.ok((timeline(entry,900,false)[key]-timeline(entry,899,false)[key]) < .0001);
  assert.ok(timeline(entry,120,false).softness>1);
  assert.equal(timeline(entry,900,false).softness,0);
  assert.equal(timeline(entry,65,true).softness,0);
});

test('capture-edge intersections remain rounded with finite exterior distances', () => {
  const field = build(300,200,[[0,0,300,200]]);
  assert.ok(distance(field,300,200,1,1) > 0, 'the final corner must turn inward, not be cropped square');
  assert.ok(distance(field,300,200,8,8) < 0, 'the capture corner retains the compact eight-pixel radius');
  assert.ok(distance(field,300,200,150,15) < 0);
  assert.ok(distance(field,300,200,150,100) > -110, 'the crop edge has a real exterior');
  const joined = build(400,300,[[0,40,150,25],[70,100,200,30],[30,170,70,25]]);
  for (const [x,y] of [[1,1],[0,60],[160,45],[275,100]]) assert.ok(Number.isFinite(distance(joined,400,300,x,y)));
});

test('fixed padding keeps remaining work nested when cells with different sizes finish', () => {
  const cells = [[40,40,120,20],[45,90,100,20],[280,160,60,90]];
  const before = build(480,360,cells), after = build(480,360,cells.slice(2),before.padding);
  assert.equal(after.padding,before.padding);
  for(let y=0;y<360;y+=5)for(let x=0;x<480;x+=5) {
    if(distance(after,480,360,x,y)<-1)assert.ok(distance(before,480,360,x,y)<1, 'completion must not expand another island');
  }
});
