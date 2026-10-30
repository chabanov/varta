// Minimal Wasm binary reader for pre-instantiation validation. JavaScript's
// WebAssembly API reports no import signatures, memory maxima or custom sections
// by content, so the host reads the sections it must check itself.

export class WasmFormatError extends Error {}

const VALUE_TYPES = { 0x7f: "i32", 0x7e: "i64", 0x7d: "f32", 0x7c: "f64", 0x7b: "v128", 0x70: "funcref", 0x6f: "externref" };
const SECTION = { custom: 0, type: 1, import: 2, function: 3, table: 4, memory: 5, global: 6, export: 7, start: 8, element: 9, code: 10, data: 11, dataCount: 12 };
const EXTERNAL_KIND = ["function", "table", "memory", "global"];
const WASM_MAGIC = [0x00, 0x61, 0x73, 0x6d];
const WASM_VERSION = [0x01, 0x00, 0x00, 0x00];
const MEMORY_FLAG_MAXIMUM = 0x01;
const MEMORY_FLAG_SHARED = 0x02;
const MEMORY_FLAG_64 = 0x04;
const FUNCTION_TYPE_FORM = 0x60;
const OPCODE_END = 0x0b;
const OPCODE_I32_CONST = 0x41;
const OPCODE_GLOBAL_GET = 0x23;
// Data segment modes (the segment's leading flags).
const DATA_ACTIVE = 0;
const DATA_PASSIVE = 1;
const DATA_ACTIVE_MEMORY = 2;

export class Reader {
  /** @param {Uint8Array} bytes */
  constructor(bytes, start = 0, end = bytes.length) {
    this.bytes = bytes;
    this.at = start;
    this.end = end;
  }
  byte() {
    if (this.at >= this.end) throw new WasmFormatError("unexpected end of section");
    return this.bytes[this.at++];
  }
  u32() {
    let result = 0;
    let shift = 0;
    for (;;) {
      const b = this.byte();
      result += (b & 0x7f) * 2 ** shift;
      if ((b & 0x80) === 0) break;
      shift += 7;
      if (shift > 28) throw new WasmFormatError("LEB128 u32 too long");
    }
    if (result > 0xffffffff) throw new WasmFormatError("LEB128 u32 overflow");
    return result;
  }
  s32() {
    let result = 0n;
    let shift = 0n;
    let b;
    do {
      b = this.byte();
      result |= BigInt(b & 0x7f) << shift;
      shift += 7n;
    } while (b & 0x80);
    if (b & 0x40) result -= 1n << shift;
    return Number(BigInt.asIntN(32, result));
  }
  s64() {
    let result = 0n;
    let shift = 0n;
    let b;
    do {
      b = this.byte();
      result |= BigInt(b & 0x7f) << shift;
      shift += 7n;
    } while (b & 0x80);
    if (b & 0x40) result -= 1n << shift;
    return BigInt.asIntN(64, result);
  }
  skip(count) {
    if (this.at + count > this.end) throw new WasmFormatError("unexpected end of section");
    this.at += count;
  }
  slice(count) {
    this.skip(count);
    return this.bytes.subarray(this.at - count, this.at);
  }
  name() {
    return new TextDecoder("utf-8", { fatal: true }).decode(this.slice(this.u32()));
  }
  valueType() {
    const code = this.byte();
    const type = VALUE_TYPES[code];
    if (!type) throw new WasmFormatError(`unknown value type 0x${code.toString(16)}`);
    return type;
  }
  limits() {
    const flags = this.byte();
    const min = this.u32();
    const max = flags & MEMORY_FLAG_MAXIMUM ? this.u32() : null;
    return { min, max, shared: Boolean(flags & MEMORY_FLAG_SHARED), is64: Boolean(flags & MEMORY_FLAG_64) };
  }
  constExpr() {
    const opcode = this.byte();
    let value = null;
    if (opcode === 0x41) value = this.s32();
    else if (opcode === 0x42) value = this.s64();
    else if (opcode === 0x43) this.skip(4);
    else if (opcode === 0x44) value = new DataView(this.slice(8).slice().buffer).getFloat64(0, true);
    else if (opcode === 0x23 || opcode === 0xd2) this.u32();
    else if (opcode === 0xd0) this.byte();
    else throw new WasmFormatError(`unsupported constant expression 0x${opcode.toString(16)}`);
    if (this.byte() !== OPCODE_END) throw new WasmFormatError("constant expression is not a single instruction");
    return value;
  }
}

/**
 * @param {Uint8Array} bytes
 * @returns parsed module structure; throws WasmFormatError on malformed input.
 */
export function readModule(bytes) {
  if (bytes.length < 8 || WASM_MAGIC.some((b, i) => bytes[i] !== b)) throw new WasmFormatError("not a Wasm binary");
  if (WASM_VERSION.some((b, i) => bytes[4 + i] !== b)) throw new WasmFormatError("unsupported Wasm version");
  const module = {
    types: [],
    imports: [],
    functionTypes: [],
    tables: [],
    memories: [],
    globals: [],
    exports: [],
    start: null,
    bodySizes: [],
    bodies: [],
    data: [],
    custom: [],
    bytes,
  };
  const seen = new Set();
  const reader = new Reader(bytes, 8);
  while (reader.at < bytes.length) {
    const id = reader.byte();
    const size = reader.u32();
    const end = reader.at + size;
    if (end > bytes.length) throw new WasmFormatError("section exceeds module");
    const section = new Reader(bytes, reader.at, end);
    if (id !== SECTION.custom) {
      if (seen.has(id)) throw new WasmFormatError(`duplicate section ${id}`);
      seen.add(id);
    }
    readSection(id, section, module);
    if (section.at !== end) throw new WasmFormatError(`section ${id} has trailing bytes`);
    reader.at = end;
  }
  return module;
}

function readSection(id, r, module) {
  switch (id) {
    case SECTION.custom: {
      const name = r.name();
      module.custom.push({ name, data: r.slice(r.end - r.at) });
      return;
    }
    case SECTION.type:
      for (let n = r.u32(); n > 0; n -= 1) {
        if (r.byte() !== FUNCTION_TYPE_FORM) throw new WasmFormatError("unsupported type form");
        const params = [];
        for (let p = r.u32(); p > 0; p -= 1) params.push(r.valueType());
        const results = [];
        for (let q = r.u32(); q > 0; q -= 1) results.push(r.valueType());
        module.types.push({ params, results });
      }
      return;
    case SECTION.import:
      for (let n = r.u32(); n > 0; n -= 1) {
        const moduleName = r.name();
        const name = r.name();
        const kind = EXTERNAL_KIND[r.byte()];
        const entry = { module: moduleName, name, kind };
        if (kind === "function") entry.type = r.u32();
        else if (kind === "table") (entry.element = r.valueType()), (entry.limits = r.limits());
        else if (kind === "memory") entry.limits = r.limits();
        else if (kind === "global") (entry.valueType = r.valueType()), (entry.mutable = r.byte() === 1);
        else throw new WasmFormatError("unknown import kind");
        module.imports.push(entry);
      }
      return;
    case SECTION.function:
      for (let n = r.u32(); n > 0; n -= 1) module.functionTypes.push(r.u32());
      return;
    case SECTION.table:
      for (let n = r.u32(); n > 0; n -= 1) module.tables.push({ element: r.valueType(), limits: r.limits() });
      return;
    case SECTION.memory:
      for (let n = r.u32(); n > 0; n -= 1) module.memories.push(r.limits());
      return;
    case SECTION.global:
      for (let n = r.u32(); n > 0; n -= 1) {
        const valueType = r.valueType();
        const mutable = r.byte() === 1;
        module.globals.push({ valueType, mutable, init: r.constExpr() });
      }
      return;
    case SECTION.export:
      for (let n = r.u32(); n > 0; n -= 1) {
        const name = r.name();
        const kind = EXTERNAL_KIND[r.byte()];
        if (!kind) throw new WasmFormatError("unknown export kind");
        module.exports.push({ name, kind, index: r.u32() });
      }
      return;
    case SECTION.start:
      module.start = r.u32();
      return;
    case SECTION.code:
      for (let n = r.u32(); n > 0; n -= 1) {
        const size = r.u32();
        module.bodySizes.push(size);
        module.bodies.push({ start: r.at, end: r.at + size });
        r.skip(size);
      }
      return;
    case SECTION.data:
      for (let n = r.u32(); n > 0; n -= 1) module.data.push(dataSegment(r));
      return;
    default:
      r.skip(r.end - r.at);
  }
}

/** One data segment: its mode, memory and, for an `i32.const` offset, the offset. */
function dataSegment(r) {
  const flags = r.u32();
  if (flags === DATA_PASSIVE) {
    const size = r.u32();
    r.skip(size);
    return { active: false, memory: null, offset: null, size };
  }
  if (flags !== DATA_ACTIVE && flags !== DATA_ACTIVE_MEMORY) throw new WasmFormatError(`unknown data segment flags ${flags}`);
  const memory = flags === DATA_ACTIVE_MEMORY ? r.u32() : 0;
  const opcode = r.byte();
  let offset = null;
  if (opcode === OPCODE_I32_CONST) offset = r.s32();
  else if (opcode === OPCODE_GLOBAL_GET) r.u32();
  else throw new WasmFormatError(`unsupported data offset expression 0x${opcode.toString(16)}`);
  if (r.byte() !== OPCODE_END) throw new WasmFormatError("data offset is not a single instruction");
  const size = r.u32();
  r.skip(size);
  return { active: true, memory, offset, size };
}

/** Resolves the type of a function index across imports and definitions. */
export function functionType(module, index) {
  const imported = module.imports.filter((i) => i.kind === "function");
  const typeIndex = index < imported.length ? imported[index].type : module.functionTypes[index - imported.length];
  return module.types[typeIndex];
}

/** Resolves a global index across imports and definitions. */
export function globalAt(module, index) {
  const imported = module.imports.filter((i) => i.kind === "global");
  return index < imported.length ? { ...imported[index], init: null } : module.globals[index - imported.length];
}
