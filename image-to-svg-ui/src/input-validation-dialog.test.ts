import assert from "node:assert/strict";
import test from "node:test";
import { showInputValidationDialog } from "./input-validation-dialog.ts";

// The production presenter runs against a DOM API double;native modal rendering
// and WebView accessibility still require real application acceptance.
function documentDouble(failOpen=false) {
  let active:Element|null=null;
  class Element extends EventTarget {
    children:Element[]=[];parent:Element|null=null;isConnected=false;open=false;id="";className="";textContent="";focusCount=0;
    attributes=new Map<string,string>();
    tagName:string;
    constructor(tagName:string) { super();this.tagName=tagName; }
    setAttribute(name:string,value:string){this.attributes.set(name,value);}
    connect(value:boolean){this.isConnected=value;for(const child of this.children)child.connect(value);}
    append(...elements:Element[]){for(const element of elements){element.parent=this;element.connect(this.isConnected);this.children.push(element);}}
    remove(){if(this.parent)this.parent.children=this.parent.children.filter(child=>child!==this);this.connect(false);}
    showModal(){if(failOpen)throw new Error("unavailable");this.open=true;}
    close(){if(!this.open)return;this.open=false;this.dispatchEvent(new Event("close"));}
    focus(){active=this;this.focusCount++;}
    click(){this.dispatchEvent(new Event("click"));}
  }
  const body=new Element("body");body.isConnected=true;
  const previous=new Element("button");body.append(previous);previous.focus();previous.focusCount=0;
  return {Element,previous,body,document:{body,get activeElement(){return active;},createElement:(name:string)=>new Element(name)}};
}

const copy={name:"<image>.png",title:"Check image",message:"Choose another image",primary:"Choose",close:"Close"};
async function usingDocument(run:(dom:ReturnType<typeof documentDouble>)=>Promise<void>,failOpen=false) {
  const oldDocument=globalThis.document,oldElement=globalThis.HTMLElement;
  const dom=documentDouble(failOpen);
  globalThis.document=dom.document as unknown as Document;
  globalThis.HTMLElement=dom.Element as unknown as typeof HTMLElement;
  try {await run(dom);} finally {globalThis.document=oldDocument;globalThis.HTMLElement=oldElement;}
}

for (const action of ["close","escape","choose","retry"] as const) test("dialog action: "+action,()=>usingDocument(async dom=>{
  const controller=new AbortController();
  const result=showInputValidationDialog(copy,action==="retry" ? "retry" : "choose",controller.signal);
  const dialog=dom.body.children.at(-1)!;
  assert.equal(dialog.attributes.get("role"),"alertdialog");
  assert.equal(dialog.attributes.get("aria-labelledby"),dialog.children[0].id);
  assert.equal(dialog.attributes.get("aria-describedby"),dialog.children[2].id);
  assert.equal(dialog.children[1].textContent,"<image>.png");
  if(action==="escape")dialog.dispatchEvent(new Event("cancel",{cancelable:true}));
  else dialog.children[3].children[action==="close" ? 0 : 1].click();
  assert.equal(await result,action==="close" || action==="escape" ? "dismiss" : action);
  assert.equal(dialog.isConnected,false);assert.equal(dom.previous.focusCount,1);
  controller.abort();assert.equal(dom.previous.focusCount,1);
}));

test("retiring a visible dialog dismisses,removes its abort listener and restores focus once",()=>usingDocument(async dom=>{
  const controller=new AbortController();const listeners=new Set<EventListenerOrEventListenerObject>();
  const add=controller.signal.addEventListener.bind(controller.signal),remove=controller.signal.removeEventListener.bind(controller.signal);
  controller.signal.addEventListener=((type:string,listener:EventListenerOrEventListenerObject,options?:unknown)=>{
    if(type==="abort")listeners.add(listener);add(type,listener,options as AddEventListenerOptions);
  }) as typeof controller.signal.addEventListener;
  controller.signal.removeEventListener=((type:string,listener:EventListenerOrEventListenerObject)=>{
    if(type==="abort")listeners.delete(listener);remove(type,listener);
  }) as typeof controller.signal.removeEventListener;
  const result=showInputValidationDialog(copy,"choose",controller.signal);const dialog=dom.body.children.at(-1)!;
  assert.equal(listeners.size,1);controller.abort();assert.equal(await result,"dismiss");
  assert.equal(listeners.size,0);assert.equal(dialog.isConnected,false);assert.equal(dom.previous.focusCount,1);
  dialog.children[3].children[1].click();assert.equal(dom.previous.focusCount,1);
}));

test("an already retired dialog is never mounted",()=>usingDocument(async dom=>{
  const controller=new AbortController();controller.abort();
  assert.equal(await showInputValidationDialog(copy,"retry",controller.signal),"dismiss");assert.equal(dom.body.children.length,1);
}));

test("failed modal admission removes the owned document node",()=>usingDocument(async dom=>{
  assert.equal(await showInputValidationDialog(copy,"retry",new AbortController().signal),"dismiss");
  assert.equal(dom.body.children.length,1);assert.equal(dom.previous.focusCount,1);
},true));
