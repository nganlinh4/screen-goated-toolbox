import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { setImmediate as settle } from "node:timers/promises";
import { SvgSubmissionController } from "./submission-controller.ts";
import { runInputValidation } from "./input-validation-flow.ts";
import { canActivatePrimaryAction, freshSubmissionSession } from "./explicit-submission.ts";
import { claimNextQueued } from "./queue-dispatch.ts";
import type { Item, JobStatus } from "./types.ts";

const fixture = JSON.parse(readFileSync(new URL("../../parity-fixtures/image-to-svg/input-contract.json",import.meta.url),"utf8"));
function deferred<T>() {
  let resolve!: (value:T)=>void;
  let reject!: (error:Error)=>void;
  const promise=new Promise<T>((yes,no)=>{resolve=yes;reject=no;});
  return {promise,resolve,reject};
}
const status = (stage:JobStatus["stage"], outputPath?:string): JobStatus => ({
  jobId:"native-job",stage,progressText:"",sourceImagePath:"source.png",outputDir:"output",model:"simple",backgroundMode:"opaque",outputPath,
});
function harness(options: {
  validate?: (signal:AbortSignal,current:()=>boolean)=>Promise<boolean>;
  invoke?: (command:string,args:unknown)=>Promise<unknown>;
}={}) {
  const prior:Item={id:"published",batchId:"original",path:"source.png",sourceProvenance:"surface-import",name:"source.png",
    model:"simple",backgroundMode:"opaque",outputDir:"output",stage:"done",jobId:"published-job",outputPath:"prior.svg",svgText:"<svg/>",historyId:"saved"};
  const original=structuredClone(prior);
  const item=freshSubmissionSession(prior,"submission");
  const items=[prior,item];
  assert.equal(claimNextQueued(items),item);
  const calls:Array<{command:string,args:unknown}>=[],accepted:JobStatus[]=[],renders:number[]=[];
  const controller=new SvgSubmissionController({items:()=>items,
    invoke:async<T>(command:string,args?:unknown)=>{
      calls.push({command,args});
      return (options.invoke ? await options.invoke(command,args) : command==="start_job" ? status("queued") : [status("cancelled")]) as T;
    },
    validate:(_input,signal,current)=>options.validate ? options.validate(signal,current) : Promise.resolve(true),
    accepted:(_item,value)=>accepted.push(value),render:()=>renders.push(1),failedText:()=>"failed",unconfirmedText:()=>"unconfirmed",
  });
  return {controller,item,prior,original,items,calls,accepted,renders};
}

for (const row of fixture.validationActions) test("shared validation action: "+row.name,async()=>{
  let checks=0,dialogs=0,chooses=0;
  const p=harness({validate:(signal,current)=>runInputValidation(async()=>({error:row.checks[checks++]}),
    async()=>row.actions[dialogs++],()=>{chooses++;},signal,current)});
  await p.controller.dispatch(p.item);
  assert.equal(p.calls.filter(call=>call.command==="start_job").length,row.submits ? 1 : 0);
  assert.equal(chooses,row.chooses);
  assert.deepEqual(p.prior,p.original);
  if (row.actions[0]==="dismiss") assert.equal(row.submits,fixture.dismissSubmits);
  assert.equal(fixture.preservesExistingResults,true);
});

test("Cancel retires queued validation before a native job ID exists",async()=>{
  const check=deferred<{error:string|null}>();let dialogs=0;
  const p=harness({validate:(signal,current)=>runInputValidation(()=>check.promise,async()=>{dialogs++;return "choose";},()=>assert.fail("retired picker"),signal,current)});
  const running=p.controller.dispatch(p.item);
  await p.controller.cancel(p.item);
  await running;
  assert.equal(p.item.stage,"cancelled");assert.deepEqual(p.calls,[]);
  check.resolve({error:null});await settle();
  assert.equal(dialogs,0);assert.deepEqual(p.calls,[]);assert.deepEqual(p.prior,p.original);
});

test("cancelled validation consumes a late failed bridge promise without a dialog",async()=>{
  const check=deferred<{error:string|null}>();
  const p=harness({validate:(signal,current)=>runInputValidation(()=>check.promise,async()=>assert.fail("retired dialog"),()=>assert.fail("retired picker"),signal,current)});
  const running=p.controller.dispatch(p.item);await p.controller.cancel(p.item);await running;
  check.reject(new Error("late bridge failure"));await settle();assert.deepEqual(p.calls,[]);
});

test("retirement while a dialog is open cannot choose or submit",async()=>{
  const dialog=deferred<"choose">();let entered=false;
  const p=harness({validate:(signal,current)=>runInputValidation(async()=>({error:"image_invalid"}),()=>{entered=true;return dialog.promise;},()=>assert.fail("retired picker"),signal,current)});
  const running=p.controller.dispatch(p.item);await settle();assert.equal(entered,true);
  await p.controller.cancel(p.item);await running;dialog.resolve("choose");await settle();
  assert.deepEqual(p.calls,[]);assert.deepEqual(p.prior,p.original);
});

test("late accepted start is cancelled by its exact native job identity",async()=>{
  const start=deferred<JobStatus>();
  const p=harness({invoke:async(command)=>command==="start_job" ? start.promise : [status("cancelled")]});
  const running=p.controller.dispatch(p.item);await settle();
  await p.controller.cancel(p.item);assert.equal(p.item.stage,"cancelling");
  start.resolve(status("queued"));await running;
  assert.deepEqual(p.calls.map(call=>call.command),["start_job","cancel_job"]);
  assert.deepEqual(p.calls[1].args,{jobId:"native-job"});assert.equal(p.item.stage,"cancelled");
  assert.equal(p.item.outputPath,undefined);assert.deepEqual(p.prior,p.original);
});

for (const row of fixture.cancellationOutcomes) test("shared cancellation outcome: "+row.stage,async()=>{
  const result=status(row.stage,row.keepsPublishedResult ? "committed.svg" : undefined);
  const p=harness({invoke:async()=>[result]});p.item.stage="generating";p.item.jobId="native-job";
  await p.controller.cancel(p.item);
  assert.equal(p.item.stage,row.stage);assert.equal(canActivatePrimaryAction(p.item),row.canSubmit);
  assert.equal(Boolean(p.item.outputPath),row.keepsPublishedResult);
  const current=structuredClone(p.item);const update=p.controller.applyStatus(p.item,status("generating"));
  if (row.terminal) {assert.equal(update,null);assert.deepEqual(p.item,current);}
  else assert.equal(p.item.stage,"cancelling");
  assert.deepEqual(p.prior,p.original);
});

test("a result already committed before cancellation remains published",async()=>{
  const start=deferred<JobStatus>();
  const p=harness({invoke:async(command)=>{assert.equal(command,"start_job");return start.promise;}});
  const running=p.controller.dispatch(p.item);await settle();await p.controller.cancel(p.item);
  start.resolve(status("done","committed.svg"));await running;
  assert.equal(p.item.stage,"done");assert.equal(p.item.outputPath,"committed.svg");assert.equal(p.calls.length,1);
  assert.deepEqual(p.prior,p.original);
});

test("unconfirmed cancellation stays pending and cannot enable another submission",async()=>{
  const p=harness({invoke:async()=>[]});p.item.stage="generating";p.item.jobId="native-job";
  await assert.rejects(p.controller.cancel(p.item),/unconfirmed/);
  assert.equal(p.item.stage,"cancelling");assert.equal(p.item.error,"unconfirmed");
  assert.equal(canActivatePrimaryAction(p.item),false);assert.deepEqual(p.prior,p.original);
});

test("changed request fields retire the old validation and require a new explicit press",async()=>{
  const valid=deferred<boolean>();const p=harness({validate:()=>valid.promise});
  const running=p.controller.dispatch(p.item);p.item.path="replacement.png";
  valid.resolve(true);await running;
  assert.deepEqual(p.calls,[]);assert.equal(p.item.stage,"draft");assert.equal(p.item.submitted,false);
  assert.equal(p.item.path,"replacement.png");assert.deepEqual(p.prior,p.original);
});

test("surface disposal retires a late start without publishing into the closed surface",async()=>{
  const start=deferred<JobStatus>();
  const p=harness({invoke:async(command)=>command==="start_job" ? start.promise : [status("cancelled")]});
  const running=p.controller.dispatch(p.item);await settle();p.controller.dispose();
  start.resolve(status("queued"));await running;
  assert.deepEqual(p.calls.map(call=>call.command),["start_job","cancel_job"]);
  assert.equal(p.accepted.length,0);assert.equal(p.renders.length,0);assert.deepEqual(p.prior,p.original);
});
