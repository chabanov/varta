import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { registry } from "../../../varta-common/browser/registry.mjs";
import { encodeViewBatch, decodeViewBatch } from "./view-codec.mjs";
import { createViewModel } from "./view-model.mjs";
import { encodeViewEvent } from "./view-dom.mjs";
import { createViewEngine, ENGINE_STATUS } from "./view-engine.mjs";

// The style engine's verdict on one value or stylesheet, in a fresh session.
const engineStyle = (property, value) => { const e = createViewEngine(viewEngine); e.begin(); e.create(2, DIV); e.insert(1, 2, 0); return e.setStyle(2, property, value); };
const engineSheet = (text) => { const e = createViewEngine(viewEngine); e.begin(); return e.stylesheet(1, text); };

import { viewEngine } from "./view-engine.node.mjs";
const V = registry.view;
const DIV = V.tags.find((row) => row.name === "div").code;
const batch = (...ops) => encodeViewBatch(ops);
const create = (node, tag = "div") => ({ name: "create_element", node, tag });
const attribute = (node, text) => ({ name: "set_attribute", node, attribute: "id", text });
test('stylesheet transform functions preserve the registered spelling', () => {
  const m=createViewModel({viewEngine});
  m.commit(batch({name:'stylesheet',sheet:1,text:'.x { transform: translateY(1px); }'}));
  assert.throws(()=>m.commit(batch({name:'stylesheet',sheet:2,text:'.x { transform: translateY(bad); }'})),/stylesheet value/);
});
test("whole-batch validation rolls back fields, attributes, listeners and new nodes", () => {
  const m = createViewModel({viewEngine}); m.commit(batch(create(2), create(3, "input")));
  const before = m.snapshot();
  assert.throws(() => m.commit(batch(attribute(2, "same"), attribute(3, "same"))), /duplicate id attribute/);
  assert.deepEqual(m.snapshot(), before);
  assert.throws(() => m.commit(batch(attribute(2, "changed"), { name: "set_value", node: 3, text: "changed", base_seq: 0 }, create(4), { name: "remove", node: 999 })), /unknown node/);
  assert.deepEqual(m.snapshot(), before);
  m.commit(batch(attribute(2, "same"), attribute(3, "same"), { name: "remove_attribute", node: 2, attribute: "id" }));
  assert.equal(m.snapshot().nodes.find((node) => node.id === 3).attributes.id, "same");
});
test("decoder rejects hostile lengths, codes, padding, surrogates and trailing bytes", () => {
  const valid = batch(create(2));
  for (let length = 0; length < valid.length; length++) assert.throws(() => decodeViewBatch(valid.subarray(0, length)), /invalid view batch/);
  for (const mutate of [(bytes) => { bytes[0]++; }, (bytes) => { bytes[2] = 1; }, (bytes) => { bytes[8] = 255; }, (bytes) => { bytes[9] = 1; }, (bytes) => { bytes[16] = 255; }, (bytes) => { bytes[18] = 1; }]) { const bytes = valid.slice(); mutate(bytes); assert.throws(() => decodeViewBatch(bytes), /invalid view batch/); }
  assert.throws(() => decodeViewBatch(Uint8Array.from([...valid, 0])), /trailing bytes/);
  for (const text of ["\0", "\uD800", "\uDC00"]) assert.throws(() => decodeViewBatch(batch({ name: "create_text", node: 3, text })), /NUL|string|surrogate/);
  assert.equal(decodeViewBatch(batch({ name: "create_text", node: 3, text: "\uFEFF😀" }))[0].text, "\uFEFF😀");
});
test("shared tag content rules and bit-exact zero command arguments fail closed", () => {
  const m = createViewModel({viewEngine}); m.commit(batch(create(2, "input"), create(3, "select"), create(4, "option"), { name: "create_text", node: 5, text: "text" }));
  for (const parent of [2, 3]) assert.throws(() => m.commit(batch({ name: "insert", parent, node: 5, before: 0 })), /child not allowed/);
  m.commit(batch({ name: "insert", parent: 3, node: 4, before: 0 }, { name: "insert", parent: 4, node: 5, before: 0 }));
  assert.throws(() => m.commit(batch({ name: "command", node: 2, command: "focus", argument: -0 })), /command argument/);
});
test("inspection is detached from validation authority and CSS nesting is bounded", () => {
  const m = createViewModel({viewEngine}); m.commit(batch(create(2))); const snapshot = m.snapshot(); snapshot.nodes[0].children.push(999); snapshot.nodes[1].attributes.id = "injected";
  assert.deepEqual(m.snapshot().nodes[0].children, []); assert.deepEqual(m.snapshot().nodes[1].attributes, {});
  assert.equal(engineStyle("width", "var(".repeat(1000) + "--x" + ")".repeat(1000)), ENGINE_STATUS.invalid);
});

test("command completion and close release retained host model state",()=>{
  const m=createViewModel({viewEngine});m.commit(batch(create(2),{name:"copy_text",request:1,text:"copy"}));
  assert.deepEqual(m.snapshot().pending,[1]);m.complete(1);assert.deepEqual(m.snapshot().pending,[]);
  assert.throws(()=>m.complete(1),/unknown command result/);
  m.close();assert.deepEqual(m.snapshot().nodes,[]);assert.deepEqual(m.snapshot().sheets,{});
  assert.throws(()=>m.commit(batch(create(3))),/closed view/);
});

test('radio groups follow form owners, disconnected trees and nonempty names', () => {
  const m = createViewModel({viewEngine});
  const attr = (node, attribute, text) => ({name:'set_attribute',node,attribute,text});
  const insert = (parent,node) => ({name:'insert',parent,node,before:0});
  const checked = (node) => ({name:'set_checked',node,checked:1,base_seq:0});
  const ops = [create(2,'form'),insert(1,2),create(3,'form'),insert(1,3)];
  for(const [id,parent,name] of [[4,2,'g'],[5,2,'g'],[6,3,'g'],[7,1,'g'],[9,1,''],[10,1,''],[11,2,'other']]) {
    ops.push(create(id,'input'),attr(id,'type','radio'),attr(id,'name',name),insert(parent,id),checked(id));
  }
  ops.push(create(8,'input'),attr(8,'type','radio'),attr(8,'name','g'),checked(8));
  m.commit(batch(...ops));
  const state = () => new Map(m.snapshot().nodes.map(n=>[n.id,n]));
  assert.equal(state().get(4).checked,false);
  for(const id of [5,6,7,8,9,10,11])assert.equal(state().get(id).checked,true,`${id}`);
  m.commit(batch(insert(3,5)));
  assert.equal(state().get(5).checked,true);assert.equal(state().get(6).checked,false);
  m.commit(batch(attr(11,'name','g'),checked(4)));
  assert.equal(state().get(11).checked,false);
  m.commit(batch(attr(4,'name',''),checked(11)));
  assert.equal(state().get(4).checked,true);assert.equal(state().get(11).checked,true);
  const before=m.snapshot();
  assert.throws(()=>m.commit(batch(checked(6),attr(11,'name',''),{name:'remove',node:999})),/unknown node/);
  assert.deepEqual(m.snapshot(),before);
  m.input(6,'',true);assert.equal(state().get(5).checked,false);assert.equal(state().get(7).checked,true);
  assert.equal(state().get(5).seq,0);
  m.syncEvent({kind:'input',target:5,seq:1,flags:V.filters.flags.checked,value:''});
  assert.equal(state().get(6).checked,false);assert.equal(state().get(7).checked,true);
});

// A separate native process emits this file from varta-view's public API. No
// engine/test dependency on the prototype or on a hand-written expected tree.
const oraclePath = process.env.VARTA_VIEW_ORACLE;
if (oraclePath) {
  const oracle = JSON.parse(await readFile(oraclePath, "utf8"));
  function project(snapshot) {
    const nodes = new Map(snapshot.nodes.map((node) => [node.id, node]));
    const visit = (id) => {
      const n = nodes.get(id); if (n.kind === "text") return { id, text: n.text };
      const out = { id, tag: id === V.wire.root ? "root" : n.tag };
      for (const key of ["attributes", "style"]) if (Object.keys(n[key]).length && !(key === "style" && Object.keys(snapshot.sheets).length)) out[key] = n[key];
      if (n.class) out.class = n.class;
      const checked = n.tag === "input" && ["checkbox", "radio"].includes(n.attributes.type ?? "text");
      if (n.tag === "input" && !checked || ["textarea", "select"].includes(n.tag)) { out.value = n.value; out.seq = n.seq; }
      if (checked) { out.checked = n.checked; out.seq = n.seq; }
      if (n.tag === 'option') out.selected = Boolean(n.optionSelected);
      if (n.tag === "dialog") out.open = n.open;
      if (n.listeners.length) out.listeners = n.listeners.map((listener) => listener.event);
      if (n.children.length) out.children = n.children.map(visit); return out;
    };
    return { tree: visit(V.wire.root), title: snapshot.title, focus: snapshot.focus || null, stylesheets: Object.entries(snapshot.sheets).map(([id, text]) => ({ id: Number(id), text })) };
  }
  test("native view batches agree on acceptance, rollback and committed structural state", () => {
    for (const item of oracle.cases) {
      const m = createViewModel({viewEngine});
      for (let i = 0; i < item.batches.length; i++) {
        const result = item.results[i]; let ok = true,error;
        try { m.commit(Uint8Array.from(item.batches[i])); } catch (caught) { ok = false;error=caught; }
        assert.equal(ok, result.ok, `${item.name}/${i}: ${result.error}`);
        if(result.budget){assert.equal(error.rule,'select work budget');assert.equal(error.operation??null,result.operation,`${item.name}/${i}: budget position`);}
        if(result.radioBudget){assert.equal(error.rule,'radio work budget');assert.equal(error.operation??null,result.radioOperation,`${item.name}/${i}: radio budget position`);}
        if(result.cascadeBudget){assert.equal(error.rule,'cascade work budget');assert.equal(error.operation??null,result.cascadeOperation,`${item.name}/${i}: cascade position`);}
        if(result.expandedBudget){assert.equal(error.rule,'expanded value budget');assert.equal(error.operation??null,result.expandedOperation,`${item.name}/${i}: expanded position`);}
        if(result.depthBudget){assert.equal(error.rule,'tree depth budget');assert.equal(error.operation??null,result.depthOperation,`${item.name}/${i}: depth position`);}
        if(result.namingBudget){assert.equal(error.rule,'accessible name work budget');assert.equal(error.operation,undefined);}
        if(result.retainedBudget){assert.equal(error.rule,'retained text budget');assert.equal(error.operation,undefined,`${item.name}/${i}: final inventory`);}
        if(result.stylesheetBudget){assert.equal(error.rule,'stylesheet work budget');assert.equal(error.operation??null,result.stylesheetOperation,`${item.name}/${i}: stylesheet work`);}
        const expected = structuredClone(result.snapshot);
        if (expected.stylesheets.length) {
          const strip = (node) => { delete node.style; for (const child of node.children ?? []) strip(child); }; strip(expected.tree);
        }
        const { tree, title, focus, stylesheets } = expected;
        assert.deepEqual(project(m.snapshot()), { tree, title, focus, stylesheets }, `${item.name}/${i}`);
      }
    }
  });
  test('native stylesheet grammar and work limits agree with the browser engine',()=>{
    assert(oracle.stylesheetChecks?.length,'native stylesheet observations missing');
    for(const item of oracle.stylesheetChecks){
      const status=engineSheet(item.source);
      assert.equal(status===ENGINE_STATUS.ok,item.valid,item.source);
      assert.equal(status===ENGINE_STATUS.sheetWork,item.limit,item.source);
    }
  });
  test("all native event record kinds encode byte-identically",()=>{
    for(const event of oracle.events) assert.deepEqual(Array.from(encodeViewEvent(event.kind,event.values)),event.bytes,event.kind);
  });
  test("generated CSS grammars agree with native property validation", () => {
    const mismatches = [];
    for (const item of oracle.css) {
      const valid = engineStyle(item.property, item.value) === ENGINE_STATUS.ok;
      if (valid !== item.valid) mismatches.push({ ...item, browser: valid });
    }
    assert.deepEqual(mismatches, []);
  });
}
