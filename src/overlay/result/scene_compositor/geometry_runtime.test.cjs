const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const assert = require('node:assert/strict');
const read = name => fs.readFileSync(path.join(__dirname, name), 'utf8');

function harness() {
  const messages = [];
  const style = { transform: 'translate3d(100px,100px,0)', width: '400px', height: '200px', setProperty() {} };
  const card = { dataset: { id: '1' }, style, children: [], appendChild(child) {this.children.push(child);}, getBoundingClientRect() {
    const coords = style.transform.match(/translate3d\(([-\d.]+)px,([-\d.]+)px/);
    return { left: Number(coords[1]), top: Number(coords[2]), width: parseFloat(style.width), height: parseFloat(style.height) };
  }};
  const container = { style: {} };
  const entry = { card, visible: true, ready: true, processing: {resize() {}} };
  const cards = new Map([['1', entry]]);
  const events = {};
  const documentEvents = {};
  const frames = [];
  function listen(target, name, callback) {
    const previous = target[name];
    target[name] = (...args) => { previous?.(...args); callback(...args); };
  }
  const context = vm.createContext({
    window: { registeredWindows: { '1': {state: {groupIds: ['1']}} }, devicePixelRatio: 1,
      __SGT_CONTROL_GEOMETRY_CACHE__: () => ({clear() {}, merge() {}, get() {}}),
      ipc: {postMessage: text => messages.push(JSON.parse(text))},
      addEventListener(name, callback) {listen(events,name,callback);}, updateWindows() {},
      __SGT_TYPOGRAPHY_RESIZE__: {preview() {}} },
    document: { querySelector: selector => selector.startsWith('.result-card') ? card : null,
      querySelectorAll: () => [], getElementById: () => container,
      addEventListener(name, callback) {listen(documentEvents,name,callback);},
      createElement() {return {dataset:{},addEventListener(name,callback){this[name]=callback;}};} },
    cards, syncSourceBackdrop() {}, activateCard() {}, queueFit() {},
    setTimeout() {}, clearTimeout() {}, setResultDraggingCursor() {},
    requestAnimationFrame: callback => {frames.push(callback);return frames.length;}, cancelAnimationFrame() {},
  });
  const run = source => vm.runInContext(source, context);
  run(read('scene_command_helpers.js').replaceAll('__SGT_BOX_RADIUS_PX__', '8'));
  context.updateGeometry = model => {
    context.applyGeometry(entry, model);
    entry.visible = model.visible;
  };
  run(read('control_surface/drag_runtime.rs').split('r#"')[1].split('"#')[0]);
  run(read('host_command_runtime.js'));
  run(read('button_scene_runtime.js'));
  run(read('resize_runtime.js').replaceAll('__SGT_MIN_WINDOW_WIDTH_PX__','100').replaceAll('__SGT_MIN_WINDOW_HEIGHT_PX__','60'));
  run(read('scene_batch_runtime.js'));
  context.applyGeometry(entry, {rect: {x:100,y:100,width:400,height:200}});
  const event = {button:0,pointerId:1,clientX:100,clientY:100,
    currentTarget:{setPointerCapture() {}},preventDefault() {}};
  function start() {context.handleResultDrag(event,'1',false);}
  function move() {
    context.queueResultDragPreview({...event,clientX:180,clientY:140});
    context.renderResultDragPreview();
  }
  function settle(x,y,id=1) {
    context.window.applyHostCommand({type:'drag_settled',gesture_id:id,
      cards:[{id:1,visible:true,rect:{x,y,width:400,height:200}}]});
  }
  return {context,entry,card,messages,events,documentEvents,frames,event,start,move,settle};
}

test('focus-loss cancellation restores canonical card bounds before unlock', () => {
  const h = harness();h.start();h.move();h.events.blur();
  assert.equal(h.messages.find(m=>m.action==='result_drag_finish').cancelled,true);
  h.settle(100,100);
  assert.deepEqual(h.card.getBoundingClientRect(),{left:100,top:100,width:400,height:200});
  assert.equal(h.context.window.shouldPreserveResultDragGeometry('1'),false);
});

test('normal release uses host final coordinates without retaining a divergent preview', () => {
  const h=harness();h.start();h.move();
  h.context.finishLocalResultDrag({...h.event,type:'pointerup',clientX:180,clientY:140});
  h.settle(179,139);
  assert.equal(h.card.getBoundingClientRect().left,179);
  assert.equal(h.card.getBoundingClientRect().top,139);
});

test('stale settlement cannot alter or unlock a newer active gesture', () => {
  const h=harness();h.start();h.move();
  h.settle(0,0,99);
  assert.equal(h.card.getBoundingClientRect().left,180);
  assert.equal(h.context.window.shouldPreserveResultDragGeometry('1'),true);
  h.events.blur();h.settle(100,100);
  h.start();h.move();h.settle(0,0,1);
  assert.equal(h.card.getBoundingClientRect().left,180);
  assert.equal(h.context.window.shouldPreserveResultDragGeometry('1'),true);
});

test('duplicate settlement is ignored after completion', () => {
  const h=harness();h.start();h.move();h.events.blur();h.settle(100,100);
  h.settle(900,900);
  assert.equal(h.card.getBoundingClientRect().left,100);
});

test('cancelled resize restores size and position, while successful resize commits host bounds', () => {
  for (const cancelled of [true,false]) {
    const h=harness();
    h.context.window.__SGT_CARD_RESIZE__.attach(h.entry);
    const handle=h.card.children.find(child=>child.dataset.edge==='nw');
    handle.pointerdown({...h.event,stopPropagation(){}});
    h.documentEvents.pointermove({...h.event,clientX:120,clientY:130});
    h.frames.pop()();
    assert.equal(h.card.style.width,'380px');
    if (cancelled) h.events.blur();
    else h.documentEvents.pointerup({...h.event,type:'pointerup',clientX:120,clientY:130});
    const rect=cancelled?{x:100,y:100,width:400,height:200}:{x:120,y:130,width:380,height:170};
    h.context.window.applyHostCommand({type:'drag_settled',gesture_id:1,cards:[{id:1,visible:true,rect}]});
    assert.equal(h.card.getBoundingClientRect().left,rect.x);
    assert.equal(h.card.getBoundingClientRect().width,rect.width);
    assert.equal(h.card.getBoundingClientRect().height,rect.height);
  }
});

test('browser acknowledgement follows geometry repair, and errors never acknowledge', () => {
  const h=harness();
  h.context.window.__SGT_APPLY_SCENE_BATCH__(()=>{h.card.style.transform='translate3d(500px,500px,0)';},
    {type:'state_acknowledged',revision:4});
  assert.equal(h.card.getBoundingClientRect().left,100);
  assert.equal(h.messages.at(-1).revision,4);
  h.context.window.__SGT_APPLY_SCENE_BATCH__(()=>{throw new Error('failed operation');},
    {type:'state_acknowledged',revision:5});
  assert.equal(h.messages.at(-1).type,'command_error');
  assert(!h.messages.some(m=>m.revision===5));
});

test('geometry reconciliation respects active previews and repairs scale changes afterward', () => {
  const h=harness();h.start();h.move();
  h.context.window.__SGT_VERIFY_SCENE_GEOMETRY__();
  assert.equal(h.card.getBoundingClientRect().left,180);
  h.events.blur();h.settle(100,100);
  h.context.window.devicePixelRatio=1.25;
  h.context.window.__SGT_VERIFY_SCENE_GEOMETRY__();
  assert.equal(h.card.getBoundingClientRect().width*1.25,400);
  assert.equal(h.card.getBoundingClientRect().left*1.25,100);
});
