import { test } from "node:test";
import assert from "node:assert/strict";
import { registry } from "../../../varta-common/browser/registry.mjs";
import { createWorkerView } from "./view-worker.mjs";
import { createViewModel } from "./view-model.mjs";
import { createViewQueue } from "./view-queue.mjs";
import { encodeViewBatch, decodeViewEvent } from "./view-codec.mjs";
import { encodeViewEvent } from "./view-dom.mjs";

function state() {
  const memory = new WebAssembly.Memory({ initial: 1 }), messages = [];
  const view = createWorkerView({ send: (message) => messages.push(message), memory: () => memory, application: true });
  const commit = (operations) => { const bytes = encodeViewBatch(operations); new Uint8Array(memory.buffer).set(bytes); return view.commit(0, bytes.length, false); };
  return { view, messages, commit };
}
test("Worker rejects a bad batch before sending it and requires one commit per turn", async () => {
  const { view, commit, messages } = state();
  assert.equal(view.attached(),false);
  assert.throws(() => commit([{ name: "remove", node: 999 }]), /unknown node/); assert.equal(messages.length, 0);
  assert.equal(commit([{ name: "create_element", node: 2, tag: "div" }]), registry.view.statuses.ok);assert.equal(view.attached(),true);
  assert.throws(() => commit([]), /second commit/); assert.equal(messages.length, 1);
  let done = false; const flush = view.flush().then(() => { done = true; }); await Promise.resolve(); assert.equal(done, false);
  view.receive({ type: "view_ack", id: messages[0].id }); await flush; assert.equal(done, true);
  view.begin(); commit([]); view.receive({ type: "view_ack", id: messages[1].id }); await view.flush(); view.close();
});

test('select identity is deferred with its checked envelope and malformed metadata has no effect',async()=>{
  const {view,commit,messages}=state();
  const operations=[{name:'create_element',node:2,tag:'select'},{name:'insert',parent:1,node:2,before:0}];
  for(const id of [3,4])operations.push({name:'create_element',node:id,tag:'option'},
    {name:'set_attribute',node:id,attribute:'value',text:'same'},
    {name:'insert',parent:2,node:id,before:0});
  commit(operations);view.receive({type:'view_ack',id:messages[0].id});await view.flush();
  const input=encodeViewEvent('input',{handler:10,target:2,seq:1,value:'same'});
  view.receive({type:'view_event',id:1,bytes:input,gesture:false,selectedOption:4});
  assert.equal(view.model.snapshot().nodes.find(n=>n.id===3).optionSelected,true);
  view.take();assert.equal(view.model.snapshot().nodes.find(n=>n.id===4).optionSelected,true);
  const before=view.model.snapshot();
  view.receive({type:'view_event',id:2,bytes:input,gesture:false,selectedOption:NaN});
  await assert.rejects(view.flush(),/invalid select option envelope/);
  assert.deepEqual(view.model.snapshot(),before);
});

test('select work admission agrees while the local edit is deferred in the Worker',async()=>{
  const memory=new WebAssembly.Memory({initial:4}),messages=[],main=createViewModel();
  const worker=createWorkerView({send:message=>messages.push(message),memory:()=>memory,application:true});
  const commitWorker=operations=>{const bytes=encodeViewBatch(operations);new Uint8Array(memory.buffer).set(bytes);return worker.commit(0,bytes.length,false);};
  const setup=[{name:'create_element',node:2,tag:'select'},{name:'insert',parent:1,node:2,before:0}],text='X'.repeat(4096);
  for(let node=3;node<23;node++)setup.push({name:'create_element',node,tag:'option'},
    {name:'set_attribute',node,attribute:'value',text},{name:'insert',parent:2,node,before:0});
  main.commit(encodeViewBatch(setup));commitWorker(setup);worker.receive({type:'view_ack',id:1});await worker.flush();worker.begin();
  main.input(2,text,false,true,4);
  worker.receive({type:'view_event',id:1,bytes:encodeViewEvent('input',{handler:10,target:2,seq:1,value:text}),selectedOption:4,gesture:false});
  assert.equal(main.snapshot().nodes.find(n=>n.id===2).seq,1);
  assert.equal(worker.model.snapshot().nodes.find(n=>n.id===2).seq,0);
  const outcome=run=>{try{run();return {ok:true};}catch(error){return {ok:false,rule:error.rule,operation:error.operation??null};}};
  const excessive=Array.from({length:128},(_,i)=>({name:'set_value',node:2,text:`missing${i}`,base_seq:0}));
  const pending=outcome(()=>commitWorker(excessive));
  assert.equal(pending.rule,'select work budget');
  assert.deepEqual(outcome(()=>main.commit(encodeViewBatch(excessive))),pending);
  assert.equal(messages.length,1,'rejection must not send a partial batch');
  assert.equal(worker.take().id,1,'rejection must retain the deferred event');
  assert.deepEqual(worker.model.snapshot(),main.snapshot());
  const ordinary=excessive.slice(0,100);
  main.commit(encodeViewBatch(ordinary));commitWorker(ordinary);worker.receive({type:'view_ack',id:2});await worker.flush();
  assert.deepEqual(worker.model.snapshot(),main.snapshot());worker.close();
});
test("Worker asset denial is checked before a commit or a parent effect", () => {
  const { commit, messages } = state();
  assert.throws(() => commit([{ name: "create_element", node: 2, tag: "img" }, { name: "set_attribute", node: 2, attribute: "src", text: "image.png" }]), /unpackaged view asset/);
  assert.equal(messages.length, 0);
});
test("event records are checked, deferred and never reenter the guest", async () => {
  const { view } = state(); const bytes = encodeViewEvent("click", { handler: 1, target: 2 });
  const waiting = view.wait(Infinity); view.receive({ type: "view_event", id: 1, bytes, gesture: true }); await waiting;
  assert.equal(view.take().gesture, true); assert.equal(view.take(), undefined);
  const input = encodeViewEvent("input", { handler: 1, target: 2, value: "😀", seq: 3 });
  view.receive({ type: "view_event", id: 2, bytes: input, gesture: true }); assert.equal(view.take().gesture, false);
  const invalid = bytes.slice(); invalid[0]++; assert.throws(() => decodeViewEvent(invalid), /invalid event record/);
  view.receive({ type: "view_event", id: 3, bytes: invalid }); await assert.rejects(view.flush(), /invalid event record/);
});
test("parent queue replaces coalescible records in place and fails bounded overflow", () => {
  const queue = createViewQueue();
  queue.push({ handler: 1, coalescible: true, gesture: false, value: "old" }); queue.push({ handler: 2, coalescible: false, value: "second" });
  assert.equal(queue.push({ handler: 1, coalescible: true, gesture: true, value: "latest" }), "coalesced");
  assert.equal(queue.length, 2); assert.deepEqual(queue.shift(), { handler: 1, coalescible: true, gesture: true, value: "latest" }); assert.equal(queue.shift().handler, 2);
  for (let i = 0; i < registry.view.limits.events; i++) queue.push({ handler: i, coalescible: false });
  assert.equal(queue.push({ handler: 1000, coalescible: true }), "dropped"); assert.throws(() => queue.push({ handler: 1000, coalescible: false }), /overflow/);
  queue.clear(); assert.equal(queue.length, 0);
});

test("invalid arrival clocks and duplicate transport ids fail closed",async()=>{
  const {view}=state(),bytes=encodeViewEvent("click",{handler:1,target:2});
  view.receive({type:"view_event",id:1,bytes,gesture:true,arrived:NaN});
  await assert.rejects(view.flush(),/invalid event envelope/);
  const next=state().view;next.receive({type:"view_event",id:1,bytes,gesture:false,arrived:1});next.take();
  next.receive({type:"view_event",id:1,bytes,gesture:false,arrived:2});await assert.rejects(next.flush(),/unexpected view event/);
});

test('radio work admission is independent of deferred checked state and edit counters',async()=>{
  const memory=new WebAssembly.Memory({initial:4}),messages=[],main=createViewModel();
  const worker=createWorkerView({send:message=>messages.push(message),memory:()=>memory,application:true});
  const commitWorker=ops=>{const bytes=encodeViewBatch(ops);new Uint8Array(memory.buffer).set(bytes);return worker.commit(0,bytes.length,false);};
  const setup=[],name='Ж'.repeat(512);
  for(let node=2;node<102;node++)setup.push({name:'create_element',node,tag:'input'},
    {name:'set_attribute',node,attribute:'type',text:'radio'},{name:'set_attribute',node,attribute:'name',text:name},
    {name:'insert',parent:1,node,before:0});
  main.commit(encodeViewBatch(setup));commitWorker(setup);worker.receive({type:'view_ack',id:1});await worker.flush();worker.begin();
  main.input(2,'',true);
  worker.receive({type:'view_event',id:1,bytes:encodeViewEvent('input',{handler:10,target:2,seq:1,flags:registry.view.filters.flags.checked}),gesture:false});
  assert.equal(main.snapshot().nodes.find(n=>n.id===2).seq,1);
  assert.equal(worker.model.snapshot().nodes.find(n=>n.id===2).seq,0);
  const outcome=run=>{try{run();return {ok:true};}catch(error){return {ok:false,rule:error.rule,operation:error.operation};}};
  for(const kind of ['set_checked','set_attribute']){
    const excessive=Array.from({length:128},()=>kind==='set_checked'?{name:kind,node:2,checked:1,base_seq:0}:{name:kind,node:2,attribute:'type',text:'radio'});
    const pending=outcome(()=>commitWorker(excessive));assert.equal(pending.rule,'radio work budget');
    assert.deepEqual(outcome(()=>main.commit(encodeViewBatch(excessive))),pending);
  }
  assert.equal(messages.length,1,'no rejected batch reaches the parent');
  assert.equal(worker.take().id,1,'deferred edit is retained after rejection');
  assert.deepEqual(worker.model.snapshot(),main.snapshot());
  const ordinary=[{name:'set_checked',node:3,checked:1,base_seq:0}];main.commit(encodeViewBatch(ordinary));commitWorker(ordinary);
  worker.receive({type:'view_ack',id:2});await worker.flush();assert.deepEqual(worker.model.snapshot(),main.snapshot());worker.close();
});

test('retained text admission and field capacity agree across a deferred local edit',async()=>{
  const memory=new WebAssembly.Memory({initial:4}),messages=[],main=createViewModel();
  const worker=createWorkerView({send:message=>messages.push(message),memory:()=>memory,application:true});
  const commitWorker=ops=>{const bytes=encodeViewBatch(ops);new Uint8Array(memory.buffer).set(bytes);return worker.commit(0,bytes.length,false);};
  const {retainedTextBytes,fieldValueReservationBytes,valueBytes}=registry.view.limits;
  const fields=Math.floor(retainedTextBytes/fieldValueReservationBytes),remaining=retainedTextBytes-fields*fieldValueReservationBytes,setup=[];
  for(let node=2;node<2+fields;node++)setup.push({name:'create_element',node,tag:'input'},{name:'insert',parent:1,node,before:0});
  setup.push({name:'create_text',node:1000,text:'X'.repeat(remaining)},{name:'insert',parent:1,node:1000,before:0});
  main.commit(encodeViewBatch(setup));commitWorker(setup);worker.receive({type:'view_ack',id:1});await worker.flush();worker.begin();
  const value='界'.repeat(valueBytes/2);main.input(2,value,false);
  worker.receive({type:'view_event',id:1,bytes:encodeViewEvent('input',{handler:10,target:2,seq:1,value}),gesture:false});
  const outcome=run=>{try{run();return {ok:true};}catch(error){return {ok:false,rule:error.rule,operation:error.operation};}};
  const excessive=[{name:'set_text',node:1000,text:'X'.repeat(remaining-1)},{name:'set_title',text:'界'}];
  const pending=outcome(()=>commitWorker(excessive));assert.equal(pending.rule,'retained text budget');
  assert.deepEqual(outcome(()=>main.commit(encodeViewBatch(excessive))),pending);assert.equal(messages.length,1);
  const ordinary=[{name:'set_value',node:2,text:'guest',base_seq:0},{name:'set_text',node:1000,text:'X'.repeat(remaining-1)},{name:'set_title',text:'x'}];
  main.commit(encodeViewBatch(ordinary));commitWorker(ordinary);
  assert.equal(worker.model.snapshot().nodes.find(n=>n.id===2).value,'guest');
  assert.equal(main.snapshot().nodes.find(n=>n.id===2).value,value);
  worker.receive({type:'view_ack',id:2});await worker.flush();assert.equal(worker.take().id,1);
  assert.deepEqual(worker.model.snapshot(),main.snapshot());worker.close();
});

test('Worker rejects excessive stylesheet expansion before sending any batch',async()=>{
  const {view,commit,messages}=state();
  const css='@media '+Array(500).fill('(min-width:0px)').join(' and ')+'{'+'.a{}'.repeat(1100)+'}';
  const before=view.model.snapshot();
  assert.throws(()=>commit([{name:'set_title',text:'after'},{name:'stylesheet',sheet:1,text:css}]),error=>error.rule==='stylesheet work budget'&&error.operation===1);
  assert.equal(messages.length,0);assert.deepEqual(view.model.snapshot(),before);
  commit([{name:'stylesheet',sheet:1,text:'.a { color:#abc }'}]);
  view.receive({type:'view_ack',id:1});await view.flush();assert.equal(messages.length,1);view.close();
});


test('Worker rejects excessive tree depth before posting a batch and retains the prior model',async()=>{
 const {view,commit,messages}=state();
 commit([{name:'set_title',text:'before'}]);view.receive({type:'view_ack',id:messages[0].id});await view.flush();view.begin();
 const before=view.model.snapshot(),ops=[{name:'set_title',text:'after'}];
 for(let i=0;i<=registry.view.limits.treeDepth;i++)ops.push({name:'create_element',node:i+2,tag:'span'},
   {name:'insert',parent:i+1,node:i+2,before:0});
 assert.throws(()=>commit(ops),e=>e.rule==='tree depth budget'&&e.operation===2*(registry.view.limits.treeDepth+1));
 assert.equal(messages.length,1);assert.deepEqual(view.model.snapshot(),before);
 commit([{name:'set_title',text:'recovered'}]);view.receive({type:'view_ack',id:messages[1].id});await view.flush();
 assert.equal(view.model.snapshot().title,'recovered');view.close();
});


test('Worker rejects a transient excessive cascade before commands and before posting any batch',async()=>{
 const {view,commit,messages}=state();commit([{name:'set_title',text:'before'},{name:'create_element',node:2,tag:'button'},
 {name:'insert',parent:1,node:2,before:0}]);view.receive({type:'view_ack',id:messages[0].id});await view.flush();view.begin();
 const before=view.model.snapshot(),ops=[{name:'stylesheet',sheet:1,text:'.a{}'.repeat(1338)}];
 for(let id=3;id<131;id++)ops.push({name:'create_element',node:id,tag:'span'},{name:'set_class',node:id,text:'a'},{name:'insert',parent:1,node:id,before:0});
 const at=ops.length;ops.push({name:'command',node:2,command:'focus',argument:0},{name:'stylesheet',sheet:1,text:''});
 assert.throws(()=>commit(ops),e=>e.rule==='cascade work budget'&&e.operation===at);
 assert.equal(messages.length,1);assert.deepEqual(view.model.snapshot(),before);
 commit([]);view.receive({type:'view_ack',id:messages[1].id});await view.flush();view.close();
});
