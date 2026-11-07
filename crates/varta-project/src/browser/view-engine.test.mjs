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
// The packaged faces and the browser's measurements of the Rust test
// (`view_engine_tests::FACES` and `measures`), so the font-relative units
// resolve from the same fonts in every host.
const fixture = (name) => readFileSync(new URL(`../../../varta-style/tests/fixtures/${name}`, import.meta.url));
const FACES = [["Varta Metrics", fixture("metrics.ttf"), ["monospace"]], ["Varta Metrics", fixture("metrics-bold.ttf"), []]];
function fontMetrics(kind, family, weight, italic, size) {
  const rows = { serif: [0.45, 0.66, 0.5, 1.15], "sans-serif": [0.52, 0.72, 0.56, 1.15], "system-ui": [0.53, 0.7, 0.62, 1.21] };
  const row = kind === "default" ? rows.serif : kind === "generic" ? rows[family] : undefined;
  if (row === undefined) return undefined;
  const [ex, cap, ch, line] = row, bold = weight >= 600 ? 1.05 : 1, slant = italic ? 1.02 : 1;
  return [ex * size * bold, cap * size, ch * size * bold * slant, size, Math.round(line * size)];
}
function model() {
  const created = createViewModel({ viewEngine, fontMetrics });
  for (const [family, bytes, generics] of FACES) assert.equal(created.engine.face(family, bytes, generics), 0);
  return created;
}

test("the browser view model decides the cross-host corpus as the native renderer does", () => {
  let accepted = 0, refused = 0;
  for (const item of corpus) {
    const view = model();
    const ops = [];
    let parent = 1;
    item.classes.forEach((klass, index) => {
      const node = index + 2;
      ops.push({ name: "create_element", node, tag: item.tags?.[index] ?? "div" }, { name: "insert", parent, node, before: 0 }, { name: "set_class", node, text: klass });
      parent = node;
    });
    for (const [index, attribute, text] of item.attributes ?? []) ops.push({ name: "set_attribute", node: index + 2, attribute, text });
    for (const [index, text] of item.values ?? []) ops.push({ name: "set_value", node: index + 2, text, base_seq: 0 });
    for (const index of item.checked ?? []) ops.push({ name: "set_checked", node: index + 2, checked: 1, base_seq: 0 });
    ops.push({ name: "stylesheet", sheet: 1, text: item.sheet });
    for (const [property, text] of item.style) ops.push({ name: "set_style", node: parent, property, text });
    const record = expected[item.name];
    assert(record, `${item.name}: no native verdict`);
    const nativeAccepts = record.statuses.every((status) => status === 0);
    let browserAccepts = true;
    try { view.commit(encodeViewBatch(ops)); } catch { browserAccepts = false; }
    assert.equal(browserAccepts, nativeAccepts, `${item.name}: browser ${browserAccepts}, native ${nativeAccepts}`);
    if (!browserAccepts) { refused++; continue; }
    accepted++;
    view.engine.environment(WIDTH, HEIGHT, ROOT_FONT, ROOT_FONT, 0);
    assert.deepEqual(Object.fromEntries(view.engine.computed(parent)), record.computed, `${item.name}: computed values`);
    if (item.pseudo) assert.deepEqual(Object.fromEntries(view.engine.computedPseudo(parent, item.pseudo)), record.pseudo, `${item.name}: ::${item.pseudo} values`);
  }
  assert(accepted > 200 && refused > 20, `${accepted} accepted, ${refused} refused`);
});

test("packaged faces, the default family and the bridge's measurements give the font units", () => {
  const view = model();
  const asked = [];
  const measured = createViewModel({ viewEngine, fontMetrics: (...query) => { asked.push(query); return fontMetrics(...query); } });
  for (const [family, bytes, generics] of FACES) measured.engine.face(family, bytes, generics);
  assert.throws(() => measured.engine.face("X", FACES[0][1], ["cursive"]), /unknown generic family/);
  const ops = [{ name: "create_element", node: 2, tag: "div" }, { name: "insert", parent: 1, node: 2, before: 0 }, { name: "set_style", node: 2, property: "width", text: "2ex" }];
  for (const target of [view, measured]) target.commit(encodeViewBatch(ops));
  // Without a default family the browser's default font is measured.
  assert.equal(measured.engine.computed(2).get("width"), "14.4px");
  assert.deepEqual(asked.at(-1), ["default", "", 400, false, 16]);
  // With one, the packaged face's own x-height (520 of 1000 units).
  assert.equal(measured.engine.defaultFamily("Varta Metrics"), 0);
  assert.equal(measured.engine.computed(2).get("width"), "16.64px");
  // A Worker's model measures nothing: the CSS Values 4 fallback of 0.5em.
  const worker = createViewModel({ viewEngine });
  worker.commit(encodeViewBatch(ops));
  assert.equal(worker.engine.computed(2).get("width"), "16px");
  assert.equal(view.engine.computed(2).get("width"), "14.4px");
});

test("refusals happen before any effect and keep the committed engine state", () => {
  const model = createViewModel({ viewEngine });
  model.commit(encodeViewBatch([{ name: "create_element", node: 2, tag: "div" }, { name: "insert", parent: 1, node: 2, before: 0 }, { name: "set_class", node: 2, text: "sm:flex" }, { name: "stylesheet", sheet: 1, text: ".sm\\:flex { color: oklch(0.5 0.1 10) }" }]));
  const before = Object.fromEntries(model.engine.computed(2));
  assert.throws(() => model.commit(encodeViewBatch([{ name: "stylesheet", sheet: 1, text: ".sm\\:flex { color: #fff }" }, { name: "stylesheet", sheet: 2, text: ".b:popover-open {}" }])), /stylesheet value/);
  assert.deepEqual(Object.fromEntries(model.engine.computed(2)), before);
  assert.throws(() => model.commit(encodeViewBatch([{ name: "set_class", node: 2, text: "a\u0001" }])), /class value/);
  assert.throws(() => model.commit(encodeViewBatch([{ name: "set_style", node: 2, property: "border-radius", text: "calc(100% - 1rem)" }])), /style value/);
});
