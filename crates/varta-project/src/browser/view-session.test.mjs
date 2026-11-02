import { test } from "node:test";
import assert from "node:assert/strict";
import { registry } from "../../../varta-common/browser/registry.mjs";
import { createWorkerView } from "./view-worker.mjs";
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
