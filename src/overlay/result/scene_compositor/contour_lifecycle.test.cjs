const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

function harness() {
  let time = 0, worker, nextFrame = 0;
  const frames = new Map(), jobs = [], events = [], draws = [], uploads = [];
  const element = () => ({style:{},appendChild(){},remove(){},setAttribute(){},addEventListener(){}});
  const window = {
    addEventListener(){}, __SGT_BUILD_PROCESSING_FIELD__: function buildProcessingField(){},
    ipc:{postMessage(message){events.push(JSON.parse(message));}},
    __SGT_PROCESSING_GPU__: () => ({
      upload(field,now){uploads.push({field,now});}, draw(...args){draws.push(args);}, destroy(){}
    })
  };
  const scope = vm.createContext({window,document:{createElement:element,body:element()},
    performance:{now:()=>time},devicePixelRatio:1,matchMedia:()=>({matches:false,addEventListener(){}}),
    requestAnimationFrame:callback=>{frames.set(++nextFrame,callback);return nextFrame;},
    cancelAnimationFrame:id=>frames.delete(id),URL:{createObjectURL:()=>'',revokeObjectURL(){}},Blob:class{},
    Worker:class{constructor(){worker=this;}postMessage(job){jobs.push(job);}terminate(){this.terminated=true;}}
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname,'contour_runtime.js'),'utf8'),scope);
  const effect={id:1,rect:{x:0,y:0,width:600,height:400},revision:0,elapsed_ms:0,cells:[],closing:false,finish:false};
  return {
    jobs,events,draws,uploads,
    apply(update={}){Object.assign(effect,update);window.__SGT_PROCESSING_CONTOURS__.apply([effect]);},
    tick(now){time=now;const callbacks=[...frames.values()];frames.clear();callbacks.forEach(callback=>callback(now));},
    deliver(job=jobs.at(-1)){worker.onmessage({data:{id:job.id,revision:job.revision,field:{padding:24},duration:10}});},
    get stopped(){return worker?.terminated;}
  };
}

test('bursts keep one active job and the latest destination without resetting the morph', () => {
  const h=harness();h.apply();h.tick(200);
  h.apply({revision:1,cells:[[20,20,100,20],[20,80,100,20]]});
  for(let revision=2;revision<=100;revision++)h.apply({revision,cells:[[20,80,100,20]]});
  assert.equal(h.jobs.length,1);
  h.deliver(h.jobs[0]);assert.equal(h.jobs.length,2);assert.equal(h.jobs[1].revision,100);
  assert.equal(h.jobs[1].fixedPadding,24);
  h.tick(600);const before=h.draws.at(-1)[2];assert.ok(before>0);
  h.deliver(h.jobs[1]);h.tick(620);
  assert.ok(h.draws.at(-1)[2]>before,'new geometry must not restart the capture transition');
  assert.equal(h.uploads.length,2);
  h.deliver(h.jobs[0]);assert.equal(h.uploads.length,2,'stale result cannot replace newer geometry');
  h.apply();assert.equal(h.jobs.length,2,'duplicate snapshot must not rebuild');
  h.apply({revision:1,cells:[[20,20,100,20],[20,80,100,20]]});
  assert.equal(h.jobs.length,2,'older snapshot must not restore finished work');
});

test('empty work fades immediately and cannot be revived by a late field or a replay', () => {
  const h=harness();h.apply({revision:1,cells:[[20,20,100,20]]});h.tick(200);
  h.deliver();h.tick(400);
  h.apply({revision:2,cells:[[20,20,60,20]]});
  h.apply({revision:3,cells:[]});h.tick(500);const opacity=h.draws.at(-1)[4];
  h.deliver();h.tick(600);assert.ok(h.draws.at(-1)[4]<opacity);assert.equal(h.uploads.length,1);
  h.tick(780);assert.equal(h.draws.at(-1)[4],0);
  assert.equal(h.events.filter(e=>e.type==='processing_finished').length,0,'native ownership awaits host completion');
  h.apply({closing:true,finish:true});h.tick(800);assert.equal(h.stopped,true);
  h.apply();h.tick(1000);assert.equal(h.events.filter(e=>e.type==='processing_finished').length,1);
});

test('cancellation does not wait for geometry or accept late worker output', () => {
  const h=harness();h.apply({revision:1,cells:[[20,20,100,20]]});h.tick(200);
  h.apply({closing:true,finish:false});h.deliver();assert.equal(h.uploads.length,0);
  h.tick(430);assert.equal(h.stopped,true);
  assert.equal(h.events.filter(e=>e.type==='processing_finished').length,1);
});

test('rectangle glows allocate the shared renderer only while active and release after fading', () => {
  let now=0, created=0, released=0, next=0;
  const frames=new Map();
  const element=()=>({style:{},children:[],setAttribute(){},addEventListener(){},
    appendChild(child){this.children.push(child);},replaceChildren(){this.children=[];},
    get childElementCount(){return this.children.length;}});
  const window={__SGT_PROCESSING_GPU__:()=>{created++;return {draw(){},destroy(){released++;}};}};
  const scope=vm.createContext({window,document:{createElement:element},performance:{now:()=>now},
    matchMedia:()=>({matches:true,addEventListener(){},removeEventListener(){}}),
    requestAnimationFrame:fn=>{frames.set(++next,fn);return next;},cancelAnimationFrame:id=>frames.delete(id)});
  vm.runInContext(fs.readFileSync(path.join(__dirname,'rectangle_glow.js'),'utf8'),scope);
  const tick=time=>{now=time;const callbacks=[...frames.values()];frames.clear();callbacks.forEach(fn=>fn(time));};
  const glow=window.__SGT_CREATE_RECTANGLE_GLOW__();
  glow.resize(300,200,1.5);glow.setState(false);assert.equal(created,0);
  glow.setState(true);assert.equal(created,1);tick(200);assert.equal(frames.size,0);
  glow.setState(false);tick(250);assert.equal(released,0);
  glow.setState(true);assert.equal(created,1);tick(450);
  glow.setState(false);tick(650);assert.equal(released,1);assert.equal(glow.element.childElementCount,0);
  glow.setState(true);assert.equal(created,2);glow.destroy();assert.equal(released,2);assert.equal(frames.size,0);
});
