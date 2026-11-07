import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { encodeViewBatch } from "./view-codec.mjs";
import { createViewModel } from "./view-model.mjs";
import { viewEngine } from "./view-engine.node.mjs";

// Cross-host equality: the browser view model, deciding every style input
// with the Wasm engine, must accept, refuse and compute exactly what the
// native renderer does over the shared corpus. The native verdicts are
// `view-engine-expected.json`, which the Rust test
// `browser::view_engine_tests` regenerates from the native engine and the
// native renderer and checks against the same Wasm module.
const corpus = JSON.parse(readFileSync(new URL("./view-engine-corpus.json", import.meta.url), "utf8")).cases;
const expected = JSON.parse(readFileSync(new URL("./view-engine-expected.json", import.meta.url), "utf8"));
const WIDTH = 1600, HEIGHT = 1000, ROOT_FONT = 16;

test("the browser view model decides the cross-host corpus as the native renderer does", () => {
  let accepted = 0, refused = 0;
  for (const item of corpus) {
    const model = createViewModel({ viewEngine });
    const ops = [];
    let parent = 1;
    item.classes.forEach((klass, index) => {
      const node = index + 2;
      ops.push({ name: "create_element", node, tag: "div" }, { name: "insert", parent, node, before: 0 }, { name: "set_class", node, text: klass });
      parent = node;
    });
    ops.push({ name: "stylesheet", sheet: 1, text: item.sheet });
    for (const [property, text] of item.style) ops.push({ name: "set_style", node: parent, property, text });
    const record = expected[item.name];
    assert(record, `${item.name}: no native verdict`);
    const nativeAccepts = record.statuses.every((status) => status === 0);
    let browserAccepts = true;
    try { model.commit(encodeViewBatch(ops)); } catch { browserAccepts = false; }
    assert.equal(browserAccepts, nativeAccepts, `${item.name}: browser ${browserAccepts}, native ${nativeAccepts}`);
    if (!browserAccepts) { refused++; continue; }
    accepted++;
    model.engine.environment(WIDTH, HEIGHT, ROOT_FONT, ROOT_FONT, 0);
    assert.deepEqual(Object.fromEntries(model.engine.computed(parent)), record.computed, `${item.name}: computed values`);
  }
  assert(accepted > 200 && refused > 20, `${accepted} accepted, ${refused} refused`);
});

test("refusals happen before any effect and keep the committed engine state", () => {
  const model = createViewModel({ viewEngine });
  model.commit(encodeViewBatch([{ name: "create_element", node: 2, tag: "div" }, { name: "insert", parent: 1, node: 2, before: 0 }, { name: "set_class", node: 2, text: "sm:flex" }, { name: "stylesheet", sheet: 1, text: ".sm\\:flex { color: oklch(0.5 0.1 10) }" }]));
  const before = Object.fromEntries(model.engine.computed(2));
  assert.throws(() => model.commit(encodeViewBatch([{ name: "stylesheet", sheet: 1, text: ".sm\\:flex { color: #fff }" }, { name: "stylesheet", sheet: 2, text: ".a > .b {}" }])), /stylesheet value/);
  assert.deepEqual(Object.fromEntries(model.engine.computed(2)), before);
  assert.throws(() => model.commit(encodeViewBatch([{ name: "set_class", node: 2, text: "a\u0001" }])), /class value/);
  assert.throws(() => model.commit(encodeViewBatch([{ name: "set_style", node: 2, property: "width", text: "2ch" }])), /style value/);
});
