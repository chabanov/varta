// Test-only batch encoder for the L0 view protocol draft v0. It plays the guest's
// part until a compiler emits `view_commit`: operands are given by name, codes by
// their subset names.

const CODE_TABLE = { tag: "tags", attribute: "attributes", property: "cssProperties", event: "events", command: "commands" };

export function batchEncoder(view) {
  const layout = view.layout;
  const alignment = Number(layout["String alignment bytes"]);
  const byName = new Map(view.opcodes.map((o) => [o.name, o]));
  const ops = [];
  const builder = {
    /** Appends one operation, e.g. op("create_element", {node: 2, tag: "div"}). */
    op(name, operands) {
      const spec = byName.get(name);
      if (!spec) throw new Error(`no opcode ${name}`);
      ops.push({ spec, operands });
      return builder;
    },
    /** Appends raw bytes after the batch (for malformed-batch tests). */
    encode({ trailing = [], opcodeOverride, paddingByte = 0 } = {}) {
      const parts = [];
      const u16 = (v) => parts.push([v & 0xff, (v >> 8) & 0xff]);
      const u32 = (v) => parts.push([v & 0xff, (v >>> 8) & 0xff, (v >>> 16) & 0xff, (v >>> 24) & 0xff]);
      const f64 = (v) => {
        const b = new Uint8Array(8);
        new DataView(b.buffer).setFloat64(0, v, true);
        parts.push([...b]);
      };
      u16(Number(layout["Draft version"]));
      u16(0);
      u32(ops.length);
      ops.forEach(({ spec, operands }, index) => {
        parts.push([index === 0 && opcodeOverride !== undefined ? opcodeOverride : spec.opcode, paddingByte, 0, 0]);
        for (const operand of spec.operands) {
          const value = operands[operand.name];
          if (operand.kind === "id" || operand.kind === "u32") u32(value ?? 0);
          else if (operand.kind === "f64") f64(value ?? 0);
          else if (operand.kind === "string") {
            const text = value ?? "";
            u32(text.length * 2);
            const units = [];
            for (let i = 0; i < text.length; i += 1) units.push(text.charCodeAt(i) & 0xff, text.charCodeAt(i) >> 8);
            while (units.length % alignment) units.push(0);
            parts.push(units);
          } else {
            const table = view[CODE_TABLE[operand.name]];
            const code = value === undefined && operand.name === "key" ? 0 : typeof value === "number" ? value : operand.name === "key" ? view.keys[value]?.code : table[value];
            if (code === undefined) throw new Error(`no ${operand.name} code for ${value}`);
            u16(code);
            u16(0);
          }
        }
      });
      parts.push(trailing);
      return Uint8Array.from(parts.flat());
    },
  };
  return builder;
}
