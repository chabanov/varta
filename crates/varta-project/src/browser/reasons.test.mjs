import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";
import { registry } from "../../../varta-common/browser/registry.mjs";
const source = await readFile(new URL("./reasons.js", import.meta.url), "utf8");

function reader() {
  const L = registry.values, profile = registry.profiles.at(-1);
  const memory = new WebAssembly.Memory({ initial: Math.ceil((L.ownedBase + 4096) / L.pageBytes) });
  const module = new WebAssembly.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]));
  const context = vm.createContext({ registry, WebAssembly, TextDecoder, TextEncoder });
  vm.runInContext(source + "\nglobalThis.reader = reasonReader;", context);
  const render = context.reader(() => memory, module, profile, { maxStringBytes: 1 << 30 });
  const data = new DataView(memory.buffer);
  const handle = (ptr, len) => BigInt(len) << 32n | BigInt(ptr);
  const block = (header, len, flag, link) => {
    data.setUint32(header, Math.ceil((L.headerBytes + len) / L.alignment) * L.alignment, true);
    data.setUint32(header + L.flagsOffset, L.allocated | flag, true);
    data.setUint32(header + L.linkOffset, link, true);
    return header + L.headerBytes;
  };
  const cell = (kind, payload) => {
    const ptr = block(L.ownedBase, L.cellBytes, L.unionFlag, Number(L.references.includes(kind)));
    data.setUint32(ptr, kind, true); data.setUint32(ptr + 4, 1, true);
    data.setBigUint64(ptr + L.cellPayload, payload, true);
    return handle(ptr, L.cellBytes);
  };
  return { L, profile, render, data, memory, handle, cell, block };
}
test("primitive cells validate canonical payloads and copy bounded string diagnostics", () => {
  const { L, render, data, handle, cell } = reader();
  assert.equal(render(cell(L.tags.Null, 0n)), "null");
  assert.throws(() => render(cell(L.tags.Null, 1n)), /noncanonical/);
  assert.throws(() => render(cell(L.tags.Boolean, 2n)), /noncanonical/);
  for (let i = 0; i < 300; i++) data.setUint16(2048 + i * 2, 120, true);
  const value = cell(L.tags.String, handle(2048, 600));
  const text = render(value);
  data.setUint16(2048, 121, true);
  assert.equal(text, `"${"x".repeat(registry.scheduler.reasonUnits)}"${registry.scheduler.truncated}`);
  assert.throws(() => render(cell(L.tags.String, handle(2048, 3))), /noncanonical/);
});
test("malformed heap lengths and trace masks fail before payload traversal", () => {
  const { L, render, data, cell } = reader();
  const value = cell(L.tags.Null, 0n);
  data.setUint32(L.ownedBase + L.linkOffset, 1, true);
  assert.throws(() => render(value), /noncanonical/);
  data.setUint32(L.ownedBase + L.linkOffset, 0, true);
  data.setUint32(L.ownedBase, 0xfffffff8, true);
  assert.throws(() => render(value), /noncanonical/);
});
test("record descriptors reject huge slot/order indices before BigInt shifts", () => {
  const { L, profile, render, data, memory, handle, cell, block } = reader();
  const shape = L.recordShapes[2], desc = 512;
  const ptr = block(L.ownedBase + 64, shape.bytes, L.recordFlag, 3);
  data.setUint32(ptr, desc, true); data.setUint32(ptr + 4, 2, true);
  data.setUint32(desc, profile.valueAbi, true); data.setUint32(desc + 4, 2, true);
  for (const [i, name] of ["name", "message"].entries()) {
    const key = 256 + i * 16, bytes = new TextEncoder().encode(name), entry = desc + L.descriptorHeader + i * L.descriptorEntry;
    new Uint8Array(memory.buffer, key, bytes.length).set(bytes);
    data.setUint32(entry, key, true); data.setUint32(entry + 4, bytes.length, true);
    data.setUint32(entry + 8, i, true); data.setUint32(entry + 12, L.tags.String, true);
    data.setBigUint64(ptr + L.recordMeta + i * L.slotBytes, 0n, true);
    data.setUint32(ptr + shape.order + i * 4, i, true);
  }
  data.setUint32(ptr + shape.presence, 3, true);
  const value = cell(L.tags.Record, handle(ptr, shape.bytes));
  assert.equal(render(value), '""');
  data.setUint32(desc + L.descriptorHeader + 8, 0xffffffff, true);
  assert.throws(() => render(value), /noncanonical/);
  data.setUint32(desc + L.descriptorHeader + 8, 0, true);
  data.setUint32(ptr + shape.order, 0xffffffff, true);
  assert.throws(() => render(value), /noncanonical/);
});
