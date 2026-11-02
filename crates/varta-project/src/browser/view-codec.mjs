import { registry } from "../../../varta-common/browser/registry.mjs";

export class ViewProtocolError extends Error {
  constructor(rule, operation) {
    super(`invalid view batch${operation === undefined ? "" : ` at operation ${operation}`}: ${rule}`);
    this.rule = rule; this.operation = operation;
  }
}
const V = registry.view, W = V.wire;
const byCode = (rows) => new Map(rows.map((row) => [row.code, row]));
const opcodes = byCode(V.opcodes);
const codes = Object.fromEntries([["tag", V.tags], ["attribute", V.attributes], ["property", V.css], ["event", V.events], ["command", V.commands], ["key", V.keys]].map(([name, rows]) => [name, byCode(rows)]));

/** Fully decode before semantic validation; no borrowed guest data is retained. */
export function decodeViewBatch(bytes) {
  let at = 0, index;
  const fail = (rule) => { throw new ViewProtocolError(rule, index); };
  if (!(bytes instanceof Uint8Array) || bytes.byteLength > V.limits.batchBytes) fail("batch size");
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const need = (length) => { if (length > bytes.length - at) fail("truncated"); };
  const u16 = () => { need(2); const value = view.getUint16(at, true); at += 2; return value; };
  const u32 = () => { need(4); const value = view.getUint32(at, true); at += 4; return value; };
  const number = () => { need(8); const value = view.getFloat64(at, true); at += 8; return value; };
  const zero = (length) => { need(length); for (let i = 0; i < length; i++) if (bytes[at++] !== 0) fail("padding"); };
  const string = (limit) => {
    const length = u32(); if (length > limit) fail("string size"); if (length % 2) fail("odd string size"); need(length);
    let text;
    try { text = new TextDecoder("utf-16le", { fatal: true, ignoreBOM: true }).decode(bytes.subarray(at, at + length)); }
    catch { fail("lone surrogate"); }
    if (text.includes("\0")) fail("NUL string");
    at += length; zero((W.alignment - length % W.alignment) % W.alignment); return text;
  };
  need(W.batchHeaderBytes);
  if (u16() !== W.version) fail("version"); if (u16()) fail("flags");
  const count = u32(); if (count > V.limits.operations) fail("operation count");
  const operations = [];
  for (index = 0; index < count; index++) {
    need(W.operationHeaderBytes); const spec = opcodes.get(bytes[at++]); if (!spec) fail("unknown opcode"); zero(W.operationHeaderBytes - 1);
    const operation = { name: spec.name };
    for (const operand of spec.operands) {
      let value;
      switch (operand.kind) {
        case "id": case "u32": value = u32(); break;
        case "f64": value = number(); break;
        case "string": value = string(spec.name === "stylesheet" ? V.limits.stylesheetBytes : V.limits.valueBytes); break;
        case "code": {
          const code = u16(); zero(2);
          const row = codes[operand.name]?.get(code);
          if (!row && !(operand.name === "key" && code === V.filters.anyKey)) fail(`unknown ${operand.name}`);
          value = operand.name === "key" ? code : row.name; break;
        }
        default: fail("unknown operand kind");
      }
      operation[operand.name] = value;
    }
    operations.push(operation);
  }
  index = undefined; if (at !== bytes.length) fail("trailing bytes"); return operations;
}

/** Registry-driven encoder for host tools and acceptance tests. */
export function encodeViewBatch(operations) {
  const chunks = []; let size = W.batchHeaderBytes;
  const append = (length, write) => { size += length; if (size > V.limits.batchBytes) throw new ViewProtocolError("batch size"); const bytes = new Uint8Array(length); write(new DataView(bytes.buffer), bytes); chunks.push(bytes); };
  if (operations.length > V.limits.operations) throw new ViewProtocolError("operation count");
  for (const operation of operations) {
    const spec = V.opcodes.find((row) => row.name === operation.name); if (!spec) throw new ViewProtocolError("unknown opcode");
    append(W.operationHeaderBytes, (_view, bytes) => { bytes[0] = spec.code; });
    for (const operand of spec.operands) {
      const value = operation[operand.name];
      if (operand.kind === "string") {
        const length = value.length * 2;
        append(4 + Math.ceil(length / W.alignment) * W.alignment, (view) => { view.setUint32(0, length, true); for (let i = 0; i < value.length; i++) view.setUint16(4 + i * 2, value.charCodeAt(i), true); });
      } else if (operand.kind === "f64") append(8, (view) => view.setFloat64(0, value, true));
      else if (operand.kind === "code") {
        const code = operand.name === "key" ? value : [...codes[operand.name].values()].find((row) => row.name === value)?.code;
        if (code === undefined) throw new ViewProtocolError(`unknown ${operand.name}`);
        append(4, (view) => view.setUint16(0, code, true));
      } else append(4, (view) => view.setUint32(0, value, true));
    }
  }
  const bytes = new Uint8Array(size), view = new DataView(bytes.buffer); view.setUint16(0, W.version, true); view.setUint32(4, operations.length, true);
  let at = W.batchHeaderBytes; for (const chunk of chunks) { bytes.set(chunk, at); at += chunk.length; } return bytes;
}

/** Validate one parent-owned binary record before it reaches guest memory. */
export function decodeViewEvent(bytes) {
  const fail = () => { throw new ViewProtocolError("invalid event record"); };
  if (!(bytes instanceof Uint8Array) || bytes.length < W.eventHeaderBytes || bytes.length > W.eventHeaderBytes + W.eventStrings.length * (4 + V.limits.valueBytes)) fail();
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength), record = {};
  for (const entry of W.eventFields) {
    record[entry.name] = entry.type === "f64" ? view.getFloat64(entry.offset, true) : entry.type === "u16" ? view.getUint16(entry.offset, true) : view.getUint32(entry.offset, true);
    if (!Number.isFinite(record[entry.name])) fail();
  }
  const kind = V.events.find((kind) => kind.code === record.kind);
  if (record.version !== W.version || !kind || record.flags & ~Object.values(V.filters.flags).reduce((a, b) => a | b, 0)) fail();
  if (!Object.values(V.statuses).includes(record.status) || kind.name !== "command_result" && record.status !== V.statuses.ok) fail();
  let at = W.eventHeaderBytes;
  for (const name of W.eventStrings) {
    if (at + 4 > bytes.length) fail(); const length = view.getUint32(at, true); at += 4;
    if (length % 2 || length > V.limits.valueBytes || length > bytes.length - at) fail();
    try { record[name] = new TextDecoder("utf-16le", { fatal: true, ignoreBOM: true }).decode(bytes.subarray(at, at + length)); } catch { fail(); }
    if (record[name].includes("\0")) fail(); at += length;
    const padding = (W.alignment - length % W.alignment) % W.alignment;
    if (padding > bytes.length - at) fail(); for (let i = 0; i < padding; i++) if (bytes[at++] !== 0) fail();
  }
  if (at !== bytes.length) fail(); record.kind = kind.name; return record;
}
