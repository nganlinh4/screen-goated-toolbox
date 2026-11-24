const assert = require('node:assert/strict');
const {execFileSync} = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');
const {after, before, test} = require('node:test');

const root = path.resolve(__dirname, '../../..');
const assets = fs.mkdtempSync(path.join(os.tmpdir(), 'sgt-text-input-test-'));
const read = relative => fs.readFileSync(path.join(root, relative), 'utf8');
const asset = name => fs.readFileSync(path.join(assets, name), 'utf8');
const fixture = JSON.parse(read('parity-fixtures/preset-system/text-input-overlay.json'));
const styles = read('src/overlay/text_input/styles.rs');
const nativeCore = styles.match(/<script>([\s\S]*?)<\/script>/)[1].replaceAll('{{','{').replaceAll('}}','}');
const shim = read('mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/preset/PresetTextInputMobileShim.kt')
  .match(/= """([\s\S]*?)"""\.trimIndent\(\)/)[1];

before(() => {
  const [python, ...args] = process.platform === 'win32' ? ['py', '-3'] : ['python3'];
  const generate = [
    'import sys',
    'from pathlib import Path',
    'from generate_preset_overlay_assets import generate_text_input_assets',
    'generate_text_input_assets(*map(Path, sys.argv[1:]))',
  ].join('\n');
  execFileSync(python, [...args, '-B', '-c', generate,
    path.join(__dirname, 'styles.rs'),
    path.join(root, 'src/overlay/html_components/icons.rs'), assets,
  ], {cwd: path.join(root, 'mobile/scripts'), stdio: 'inherit'});
});

after(() => fs.rmSync(assets, {recursive: true, force: true}));

test('generated Android document retains the exact canonical event runtime',()=>{
  const generated=asset('windows_text_input.html').match(/<script>([\s\S]*?)<\/script>/)[1]
    .replace(/\{\{(?:BRIDGE_PRELUDE|EDIT_OPERATIONS|MOBILE_SHIM)}}/g,'');
  assert.equal(generated.trim(),nativeCore.trim());
});

// Exercise production event/edit scripts against a small DOM API double.
// Native windows,WebView layout,IME and touch hardware require separate acceptance.
class Element {
  constructor(id) {
    this.id = id; this.value = ''; this.selectionStart = this.selectionEnd = 0;
    this.events = new Map(); this.attributes = new Map(); this.classes = new Set();
    this.classList = {add:name=>this.classes.add(name),remove:name=>this.classes.delete(name)};
    this.textContent = this.innerText = 'Title'; this.focusCount = 0;
  }
  addEventListener(type, callback) {
    if (!this.events.has(type)) this.events.set(type, []);
    this.events.get(type).push(callback);
  }
  emit(type, fields={}) {
    const event = {target:this,button:0,shiftKey:false,defaultPrevented:false,
      preventDefault(){this.defaultPrevented=true;},...fields};
    for (const callback of this.events.get(type) || []) callback(event);
    return event;
  }
  closest(selector) {
    return selector === '#'+this.id || selector === ({closeBtn:'.close-btn',micBtn:'.mic-btn',sendBtn:'.send-btn'})[this.id] ? this : null;
  }
  setAttribute(name,value){this.attributes.set(name,value);}
  getAttribute(name){return this.attributes.get(name);}
  hasAttribute(name){return this.attributes.has(name);}
  removeAttribute(name){this.attributes.delete(name);}
  focus(){this.focusCount++;}
}

function page(platform) {
  const elements = Object.fromEntries(['editor','headerRegion','headerTitle','footerRegion','closeBtn','micBtn','sendBtn','theme-style']
    .map(name=>[name,new Element(name)]));
  const container = new Element('container');
  const document = Object.assign(new Element('document'),{
    getElementById:name=>elements[name],querySelector:selector=>selector === '.editor-container' ? container : null,
    documentElement:new Element('html'),
  });
  const messages = [],timers = [];
  const window = {__sgtDocumentNonce:'1',__sgtCancelledThrough:'0',devicePixelRatio:1,
    ipc:{postMessage:message=>messages.push(message)},setTimeout:callback=>timers.push(callback)};
  const context = vm.createContext({window,document});
  if (platform === 'android') {
    const script = asset('windows_text_input.html').match(/<script>([\s\S]*?)<\/script>/)[1];
    const parts = {BRIDGE_PRELUDE:'',EDIT_OPERATIONS:asset('windows_text_input_edits.js'),MOBILE_SHIM:shim};
    vm.runInContext(script.replace(/\{\{([A-Z_]+)}}/g,(_,key)=>{
      assert.ok(Object.hasOwn(parts,key),key);return parts[key];
    }),context);
  } else {
    vm.runInContext(nativeCore,context);
    // The same raw edit expressions are called by native renderer delivery.
    for (const [file,marker,name] of [['activation.rs','fn tick(','activate'],['editor_text.rs','fn apply_text(','apply']]) {
      const source = read('src/overlay/text_input/'+file).split(marker)[1];
      const script = source.match(/r#"([\s\S]*?)"#/)[1].replace('{payload}','payload').replaceAll('{{','{').replaceAll('}}','}');
      vm.runInContext('window.sgtText_'+name+' = function(payload){ return '+script+'; };',context);
    }
    window.sgtText_activate({nonce:'1',session:'1'});
  }
  return {window,document,elements,editor:elements.editor,container,messages,timers};
}

for (const platform of ['windows','android']) {
  for (const row of fixture.edit_cases) test(platform+': '+row.name,()=>{
    const p = page(platform);
    p.editor.value = row.initial;
    p.editor.selectionStart = row.selectionStart ?? row.initial.length;
    p.editor.selectionEnd = row.selectionEnd ?? p.editor.selectionStart;
    p.window.__sgtTextRevision = row.revision;
    const owner = row.owner || '1';
    const result = platform === 'windows' ? p.window.sgtText_apply({
      nonce:owner,session:owner,id:'1',sequence:'1',action:row.action,text:row.text || '',
      revision:row.submittedRevision || '',passive:false,snapshot:'',focus:true,
    }) : ({replace:()=>p.window.setEditorText(owner,row.text),insert:()=>p.window.insertTextAtCursor(owner,row.text),
      clear:()=>p.window.clearInput(owner,row.submittedRevision)})[row.action]();
    assert.equal(result.status,row.status);
    assert.equal(p.editor.value,row.expected);
    if (row.status === 'skipped' || row.status === 'rejected') assert.equal(p.window.__sgtTextRevision,row.revision);
  });
  for (const row of fixture.message_cases) test(platform+': '+row.name,()=>{
    const p = page(platform);
    p.editor.value = row.text;
    p.editor.selectionStart = p.editor.selectionEnd = row.caret ?? row.text.length;
    p.window.__sgtTextRevision = row.revision || '0';
    const event = p.editor.emit('keydown',{key:row.key,shiftKey:row.shift || false});
    assert.deepEqual(p.messages,row.expected === null ? [] : ['1:'+row.expected]);
    assert.equal(event.defaultPrevented,row.preventDefault);
  });
  test(platform+': every named message executes an owned event',()=>{
    for (const pattern of fixture.android_contract.required_messages) {
      const p = page(platform);
      p.editor.value='text';p.window.__sgtTextRevision='7';
      const expected=pattern.replace('<owner>','1').replace('<revision>','7').replace('<text>','text').replace('<draft>','text');
      const body=expected.slice(2);
      if (body.startsWith('submit:')) p.elements.sendBtn.emit('click');
      else if (body.startsWith('history_up:')) p.editor.emit('keydown',{key:'ArrowUp'});
      else if (body.startsWith('history_down:')) p.editor.emit('keydown',{key:'ArrowDown'});
      else if (body === 'cancel') p.editor.emit('keydown',{key:'Escape'});
      else if (body === 'close_window') p.elements.closeBtn.emit('click');
      else if (body === 'mic') p.elements.micBtn.emit('click');
      else assert.fail('Unmapped fixture message '+pattern);
      assert.deepEqual(p.messages,[expected]);
    }
  });
  test(platform+': theme,entry,exit and input revision hooks execute',()=>{
    const p=page(platform);
    for (const hook of fixture.android_contract.required_js_hooks) assert.equal(typeof p.window[hook.slice(7)],'function',hook);
    p.window.updateTheme(true);assert.equal(p.document.documentElement.getAttribute('data-theme'),'dark');
    p.window.updateTheme(false);assert.equal(p.document.documentElement.getAttribute('data-theme'),'light');
    p.window.playEntry();assert.deepEqual([...p.container.classes],['entering']);
    p.window.playExit();assert.deepEqual([...p.container.classes],['exiting']);
    p.editor.emit('input');assert.equal(p.window.__sgtTextRevision,'1');
    assert.equal(p.document.emit('contextmenu').defaultPrevented,true);
  });
  test(platform+': native dragging excludes interactive controls',()=>{
    const p=page(platform);
    for (const id of ['editor','closeBtn','micBtn','sendBtn']) p.container.emit('mousedown',{target:p.elements[id]});
    p.container.emit('mousedown',{target:p.elements.headerRegion,button:2});
    assert.deepEqual(p.messages,[]);
    p.container.emit('mousedown',{target:p.elements.headerRegion});assert.deepEqual(p.messages,['1:drag_window']);
  });
}

test('android: platform hooks preserve draft/selection and reject stale owners',()=>{
  const p=page('android');
  for (const hook of fixture.android_contract.platform_js_hooks) assert.equal(typeof p.window[hook.slice(7)],'function',hook);
  p.editor.value='newer draft';p.editor.selectionStart=2;p.editor.selectionEnd=5;
  p.window.updateInputChrome('1','<literal title>','footer','placeholder');
  assert.equal(p.elements.headerTitle.textContent,'<literal title>');
  assert.equal(p.elements.footerRegion.textContent,'footer');assert.equal(p.editor.placeholder,'placeholder');
  p.window.focusEditor('1');assert.equal(p.editor.focusCount,1);
  assert.equal(p.editor.value,'newer draft');assert.equal(p.editor.selectionStart,2);assert.equal(p.editor.selectionEnd,5);
  p.window.updateInputChrome('old','stale','stale','stale');assert.equal(p.editor.placeholder,'placeholder');
  p.window.closeWithAnimation('1');assert.equal(p.window.clearInput('1','0').status,'rejected');
  for (const timer of p.timers) timer();assert.deepEqual(p.messages,['1:input_exit_done']);
});

test('android: focus and touch drag shims retain owner-qualified messages',()=>{
  const p=page('android');
  p.editor.emit('touchstart');assert.deepEqual(p.messages,['1:request_focus']);p.messages.length=0;
  p.elements.headerRegion.emit('touchstart',{touches:[{screenX:10,screenY:20}]});
  const event=p.elements.headerRegion.emit('touchmove',{touches:[{screenX:12,screenY:23}]});
  assert.equal(event.defaultPrevented,true);
  const delta=JSON.parse(p.messages[0].slice(2));assert.equal(delta.type,'dragInputWindow');
  assert.equal(delta.dx,2*1.85);assert.equal(delta.dy,3*1.85);
  p.elements.headerRegion.emit('touchend',{changedTouches:[{screenX:12,screenY:23}]});
  assert.deepEqual(p.messages.slice(1),['1:dragAt:12,23','1:dragEnd:12,23']);
});
