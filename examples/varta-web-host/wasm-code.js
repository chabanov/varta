// Function-body scanner for frame admission (embedding.md "Function and frame
// catalogs", rules 3 and 4). It decodes every instruction so that immediates are
// never mistaken for opcodes, and fails closed on any opcode it does not know.
import { Reader, WasmFormatError } from "./wasm-binary.js";

const BLOCK_EMPTY = 0x40;
const VALUE_TYPE_CODES = new Set([0x7f, 0x7e, 0x7d, 0x7c, 0x7b, 0x70, 0x6f]);
const OP = {
  block: 0x02, loop: 0x03, if: 0x04, else: 0x05, end: 0x0b, br: 0x0c, brIf: 0x0d, brTable: 0x0e,
  call: 0x10, callIndirect: 0x11, returnCall: 0x12, returnCallIndirect: 0x13, callRef: 0x14, returnCallRef: 0x15,
  selectTyped: 0x1c, tableGet: 0x25, tableSet: 0x26, memorySize: 0x3f, memoryGrow: 0x40,
  i32Const: 0x41, i64Const: 0x42, f32Const: 0x43, f64Const: 0x44,
  refNull: 0xd0, refFunc: 0xd2, brOnNull: 0xd4, brOnNonNull: 0xd6, prefixFC: 0xfc,
};
const NO_IMMEDIATE = new Set([0x00, 0x01, 0x0f, 0x1a, 0x1b, 0xd1, 0xd3]);
const INDEX_IMMEDIATE = new Set([0x20, 0x21, 0x22, 0x23, 0x24]);
const FIRST_MEMORY_ACCESS = 0x28;
const LAST_MEMORY_ACCESS = 0x3e;
const FIRST_NUMERIC = 0x45;
const LAST_NUMERIC = 0xc4;
// 0xFC sub-opcodes: 0–7 saturating truncation, 8–11 bulk memory, 12–17 table.
const FC_LAST_TRUNC = 7;
const FC_MEMORY_INIT = 8;
const FC_DATA_DROP = 9;
const FC_MEMORY_COPY = 10;
const FC_MEMORY_FILL = 11;
const FC_FIRST_TABLE = 12;
const FC_LAST_TABLE = 17;
const FC_TABLE_IMMEDIATES = { 12: 2, 13: 1, 14: 2, 15: 1, 16: 1, 17: 1 };

function blockType(r) {
  const peek = r.bytes[r.at];
  if (peek === BLOCK_EMPTY || VALUE_TYPE_CODES.has(peek)) r.byte();
  else r.s32();
}

/**
 * @returns {{calls: number[], indirect: boolean, table: boolean}} direct call and
 * tail-call targets, and whether any indirect/reference call or table
 * instruction occurs.
 */
export function scanBody(bytes, { start, end }) {
  const r = new Reader(bytes, start, end);
  for (let groups = r.u32(); groups > 0; groups -= 1) {
    r.u32();
    r.valueType();
  }
  const out = { calls: [], indirect: false, table: false };
  let depth = 1;
  while (depth > 0) {
    const op = r.byte();
    if (op === OP.end) depth -= 1;
    else if (op === OP.block || op === OP.loop || op === OP.if) {
      blockType(r);
      depth += 1;
    } else if (op === OP.else || NO_IMMEDIATE.has(op) || (op >= FIRST_NUMERIC && op <= LAST_NUMERIC)) {
      // no immediates
    } else if (op === OP.br || op === OP.brIf || op === OP.brOnNull || op === OP.brOnNonNull || INDEX_IMMEDIATE.has(op)) r.u32();
    else if (op === OP.brTable) {
      for (let n = r.u32(); n > 0; n -= 1) r.u32();
      r.u32();
    } else if (op === OP.call || op === OP.returnCall) out.calls.push(r.u32());
    else if (op === OP.callIndirect || op === OP.returnCallIndirect) {
      r.u32();
      r.u32();
      out.indirect = true;
    } else if (op === OP.callRef || op === OP.returnCallRef) {
      r.u32();
      out.indirect = true;
    } else if (op === OP.selectTyped) {
      for (let n = r.u32(); n > 0; n -= 1) r.valueType();
    } else if (op === OP.tableGet || op === OP.tableSet) {
      r.u32();
      out.table = true;
    } else if (op >= FIRST_MEMORY_ACCESS && op <= LAST_MEMORY_ACCESS) {
      r.u32();
      r.u32();
    } else if (op === OP.memorySize || op === OP.memoryGrow) r.byte();
    else if (op === OP.i32Const) r.s32();
    else if (op === OP.i64Const) r.s64();
    else if (op === OP.f32Const) r.skip(4);
    else if (op === OP.f64Const) r.skip(8);
    else if (op === OP.refNull) r.byte();
    else if (op === OP.refFunc) r.u32();
    else if (op === OP.prefixFC) scanPrefixed(r, out);
    else throw new WasmFormatError(`unsupported opcode 0x${op.toString(16)} in a scanned body`);
  }
  if (r.at !== end) throw new WasmFormatError("function body has bytes after its final end");
  return out;
}

function scanPrefixed(r, out) {
  const sub = r.u32();
  if (sub <= FC_LAST_TRUNC) return;
  if (sub === FC_MEMORY_INIT) {
    r.u32();
    r.byte();
  } else if (sub === FC_DATA_DROP) r.u32();
  else if (sub === FC_MEMORY_COPY) {
    r.byte();
    r.byte();
  } else if (sub === FC_MEMORY_FILL) r.byte();
  else if (sub >= FC_FIRST_TABLE && sub <= FC_LAST_TABLE) {
    for (let n = FC_TABLE_IMMEDIATES[sub]; n > 0; n -= 1) r.u32();
    out.table = true;
  } else throw new WasmFormatError(`unsupported 0xfc sub-opcode ${sub}`);
}
