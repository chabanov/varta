// Tool invocation ABI v1 (level T1): strict arguments, string transfer into the
// guest and result decoding. The catalog itself is checked in validate.js.
import { parseStrictJson, exactKeys, hasLoneSurrogate, JsonNumber, JsonError } from "./strict-json.js";
import { ExecutionFailure } from "./errors.js";
import { I32_MIN, I32_MAX, utf8, strictUtf8 } from "./context.js";

const PLAIN_I32 = /^-?(0|[1-9][0-9]*)$/;
const UINT32_SPAN = 2n ** 32n;

// Failures expose no partial result and no logs (embedding.md "Envelopes").
export const toolFailure = (code, error) => ({ code, diagnostics: [], error, status: "failed" });

export function parseToolArguments(tool, text) {
  let args;
  try {
    args = parseStrictJson(text);
    exactKeys(args, tool.parameters.map((p) => p.name), [], "arguments");
  } catch (error) {
    if (error instanceof JsonError) return { error: error.message };
    throw error;
  }
  const values = [];
  for (const parameter of tool.parameters) {
    const value = args[parameter.name];
    if (parameter.valueType === "integer") {
      if (!(value instanceof JsonNumber) || !PLAIN_I32.test(value.raw) || value.raw === "-0") return { error: `${parameter.name} must be a plain integer` };
      const n = Number(value.raw);
      if (n < I32_MIN || n > I32_MAX) return { error: `${parameter.name} is outside i32` };
      values.push(n);
    } else if (parameter.valueType === "boolean") {
      if (typeof value !== "boolean") return { error: `${parameter.name} must be a boolean` };
      values.push(value ? 1 : 0);
    } else {
      if (typeof value !== "string" || hasLoneSurrogate(value)) return { error: `${parameter.name} must be a valid string` };
      const bytes = utf8.encode(value);
      if (bytes.length > Number(tool.limits.stringBytes.raw)) return { error: `${parameter.name} is too long` };
      values.push(bytes);
    }
  }
  return { values };
}

function readHandle(exec, handle, limit) {
  const unsigned = BigInt.asUintN(64, handle);
  const ptr = Number(unsigned % UINT32_SPAN);
  const len = Number(unsigned / UINT32_SPAN);
  const buffer = exec.instance.exports.memory.buffer;
  if (len > limit || ptr + len > buffer.byteLength) return null;
  return new Uint8Array(buffer, ptr, len).slice();
}

export function transferString(C, exec, value, stringLimit) {
  const x = exec.instance.exports;
  const needed = C.resultBase + value.length;
  if (needed > x.memory.buffer.byteLength) x.memory.grow(Math.ceil((needed - x.memory.buffer.byteLength) / C.pageBytes));
  new Uint8Array(x.memory.buffer).set(value, C.resultBase);
  const handle = x["varta.tool_abi.copy_string"](C.resultBase, value.length);
  const copied = readHandle(exec, handle, stringLimit);
  if (!copied || copied.length !== value.length || copied.some((b, j) => b !== value[j])) throw new ExecutionFailure("string transfer changed the input");
  if (x["varta.tool_abi.retain_string"](handle) !== handle) throw new ExecutionFailure("string retention changed the handle");
  return handle;
}

export function toolResult(exec, tool, raw, stringLimit) {
  if (tool.result === "void") return { value: null };
  if (tool.result === "integer") return { value: raw };
  if (tool.result === "boolean") return raw === 0 || raw === 1 ? { value: raw === 1 } : { error: "boolean result must be 0 or 1" };
  const out = readHandle(exec, raw, stringLimit);
  if (!out) return { error: "string result out of bounds" };
  try {
    return { value: strictUtf8.decode(out) };
  } catch {
    return { error: "string result is not UTF-8" };
  }
}
