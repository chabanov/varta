// One execution: its state, the import bodies this host implements and the
// embedding.md "Call protocol" wrapper (budget, reset, validate, authorize, effect,
// complete). Scheduler and timer bodies are in scheduler.js.
import { hasLoneSurrogate } from "./strict-json.js";
import { authorize } from "./policy.js";
import { ExecutionFailure, GuestExit, Recoverable } from "./errors.js";
import { I32_MIN, I32_MAX, MS_PER_SECOND, utf8, strictUtf8, now } from "./context.js";
import { schedulerState, schedulerImports } from "./scheduler.js";

const MATH_FUNCTIONS = [
  "acos", "acosh", "asin", "asinh", "atan", "atan2", "atanh", "cbrt", "cos", "cosh", "exp", "expm1",
  "log", "log10", "log1p", "log2", "sin", "sinh", "tan", "tanh",
];
// Imports this host implements. Everything else, including the layout-bound pure
// intrinsics the contract marks unstable and the unspecified timer operations, is
// refused before instantiation.
export const IMPLEMENTED = new Set([
  "log_i32", "log_string", "log_string16", "log_f64", "log_f64_v2", "log_undefined",
  "time_now", "env_has", "env_get", "env_print", "env_has16", "env_get16", "env_print16",
  "process_exit", "process_exit_f64", "number_remainder", "number_power", "number_random",
  "string_char_at16", "string_char_code_at16",
  "promise_handled", "promise_react", "promise_settle", "promise_unhandled", "scheduler_enqueue",
  "timer_start", "timer_start_v2", "timer_clear", "timer_take", "timer_uncaught",
  ...MATH_FUNCTIONS.map((name) => `number_${name}`),
]);
// Timer bookkeeping imports observe the deadline but are not counted host calls
// (embedding.md "Timers", Budgets); `timer_start` is counted.
const TIMER_BOOKKEEPING = new Set(["timer_clear", "timer_take", "timer_uncaught"]);
// embedding.md "Output and its budget": reference line texts.
const LOG_PREFIX = "varta log: ";
const ENV_PREFIX = "varta env: ";
const DIAGNOSTIC = {
  invalidInput: "<invalid host input>",
  tooLarge: "<result too large>",
  denied: (label, target) => `<capability denied: ${label}> (${target})`,
  io: (text) => `<host I/O error: ${text}>`,
};
const ENV_UNSET_TEXT = "environment variable not found";
// Diagnostic targets of boolean grants (embedding.md "Call protocol", denial targets).
const BOOLEAN_TARGET = { "time.now": "clock", "process.exit": "process" };
const ENV_NAME_MAX_BYTES = 256;
const UTF16_UNIT_BYTES = 2;
const lenientUtf16 = new TextDecoder("utf-16le");

/** embedding.md "Logging": `log_f64` text (shortest round-trip, no exponent). */
export function formatLegacyF64(x) {
  if (Number.isNaN(x)) return "NaN";
  if (x === Infinity) return "Infinity";
  if (x === -Infinity) return "-Infinity";
  if (Object.is(x, -0)) return "-0";
  const sign = x < 0 ? "-" : "";
  const text = String(Math.abs(x));
  const exponentAt = text.indexOf("e");
  if (exponentAt < 0) return sign + text;
  const mantissa = text.slice(0, exponentAt);
  const exponent = Number(text.slice(exponentAt + 1));
  const point = mantissa.indexOf(".");
  const digits = mantissa.replace(".", "");
  const position = (point < 0 ? mantissa.length : point) + exponent;
  if (position <= 0) return `${sign}0.${"0".repeat(-position)}${digits}`;
  if (position >= digits.length) return sign + digits + "0".repeat(position - digits.length);
  return `${sign}${digits.slice(0, position)}.${digits.slice(position)}`;
}

function encodeUtf16(text) {
  const bytes = new Uint8Array(text.length * UTF16_UNIT_BYTES);
  const view = new DataView(bytes.buffer);
  for (let i = 0; i < text.length; i += 1) view.setUint16(i * UTF16_UNIT_BYTES, text.charCodeAt(i), true);
  return bytes;
}

/** A fresh execution and its `varta` import object; `exec.instance` is set by the caller. */
export function createExecution(ctx, info, policies, options) {
  const { C } = ctx;
  const exec = {
    instance: null,
    logs: [],
    hostCalls: 0,
    outputBytes: 0,
    deadline: now() + (options.deadlineMs ?? C.deadlineMs),
    ...schedulerState(options.deadlineMs ?? C.deadlineMs),
  };
  const utf16 = info.profile?.strings === "UTF-16LE";
  const memory = () => exec.instance.exports.memory;
  const setGlobal = (name, value) => {
    const g = exec.instance.exports[name];
    if (g) g.value = value;
  };

  /** Emits one reference line; the budget is charged on its full text. */
  function emit(line) {
    exec.outputBytes += utf8.encode(line).length + 1;
    if (exec.outputBytes > C.maxOutputBytes) throw new ExecutionFailure("output budget exhausted");
    exec.logs.push(line);
    options.onLog?.(line);
  }

  function guestRange(ptr, len, limit, onInvalid) {
    const buffer = memory().buffer;
    if (ptr < 0 || len < 0 || len > limit || ptr + len > buffer.byteLength) onInvalid();
    return new Uint8Array(buffer, ptr, len).slice();
  }
  const invalid = () => {
    throw new Recoverable(C.status.InvalidInput, "invalid host input", DIAGNOSTIC.invalidInput);
  };
  const trap = (what) => () => {
    throw new ExecutionFailure(what);
  };

  function decodeUtf16(bytes, onInvalid) {
    if (bytes.length % UTF16_UNIT_BYTES) onInvalid();
    return String.fromCharCode(...new Uint16Array(bytes.buffer, bytes.byteOffset, bytes.length / UTF16_UNIT_BYTES));
  }

  /** Reads a resource name / path / URL argument with strict validation. */
  function resourceString(ptr, len) {
    const bytes = guestRange(ptr, len, C.maxInputBytes, invalid);
    if (!utf16) {
      try {
        return strictUtf8.decode(bytes);
      } catch {
        return invalid();
      }
    }
    const text = decodeUtf16(bytes, invalid);
    if (hasLoneSurrogate(text) || text.includes("\0")) invalid();
    return text;
  }

  function writeResult(text) {
    const bytes = utf16 ? encodeUtf16(text) : utf8.encode(text);
    if (bytes.length > C.maxResultBytes) throw new Recoverable(C.status.TooLarge, "result too large", DIAGNOSTIC.tooLarge);
    const mem = memory();
    const needed = C.resultBase + bytes.length;
    if (needed > mem.buffer.byteLength) {
      try {
        mem.grow(Math.ceil((needed - mem.buffer.byteLength) / C.pageBytes));
      } catch {
        throw new Recoverable(C.status.TooLarge, "result exceeds the memory maximum", DIAGNOSTIC.tooLarge);
      }
    }
    new Uint8Array(mem.buffer).set(bytes, C.resultBase);
    setGlobal("env_result_len", bytes.length);
    return C.resultBase;
  }

  function requireGrant(id, resource = "") {
    if (authorize(policies.package, policies.ceiling, id, resource)) return;
    const label = id.startsWith("env.") ? "env" : id;
    throw new Recoverable(C.status.Denied, `${id} denied`, DIAGNOSTIC.denied(label, resource || BOOLEAN_TARGET[id] || id));
  }

  function envName(ptr, len, id) {
    const name = resourceString(ptr, len);
    if (name.length === 0 || utf8.encode(name).length > ENV_NAME_MAX_BYTES || name.includes("=") || name.includes("\0")) invalid();
    requireGrant(id, name);
    return name;
  }

  const environment = options.env ?? new Map();
  const envValue = (name) => {
    if (!environment.has(name)) throw new Recoverable(C.status.Io, ENV_UNSET_TEXT, DIAGNOSTIC.io(ENV_UNSET_TEXT));
    const value = environment.get(name);
    if (utf8.encode(value).length > C.maxResultBytes) throw new Recoverable(C.status.TooLarge, "value too large", DIAGNOSTIC.tooLarge);
    return value;
  };

  function exitCode(code) {
    requireGrant("process.exit");
    if (!Number.isInteger(code) || code < I32_MIN || code > I32_MAX) invalid();
    throw new GuestExit(code);
  }

  const impl = {
    log_i32: (v) => emit(LOG_PREFIX + String(v)),
    log_f64: (v) => emit(LOG_PREFIX + formatLegacyF64(v)),
    log_f64_v2: (v) => emit(LOG_PREFIX + String(v)),
    log_undefined: (v) => {
      if (v !== 0) throw new ExecutionFailure("log_undefined argument must be 0");
      emit(`${LOG_PREFIX}undefined`);
    },
    log_string: (ptr, len) => {
      const bytes = guestRange(ptr, len, C.maxResultBytes, invalid);
      let text;
      try {
        text = strictUtf8.decode(bytes);
      } catch {
        invalid();
      }
      emit(LOG_PREFIX + text);
    },
    log_string16: (ptr, len) => {
      const fatal = trap("log_string16 received an invalid range or odd length");
      const bytes = guestRange(ptr, len, C.maxStringBytes, fatal);
      if (bytes.length % UTF16_UNIT_BYTES) fatal();
      emit(LOG_PREFIX + lenientUtf16.decode(bytes));
    },
    time_now: () => {
      requireGrant("time.now");
      const seconds = Math.floor(Date.now() / MS_PER_SECOND);
      if (seconds > I32_MAX) invalid();
      return seconds;
    },
    env_has: (ptr, len) => (environment.has(envName(ptr, len, "env.has")) ? 1 : 0),
    env_get: (ptr, len) => writeResult(envValue(envName(ptr, len, "env.get"))),
    env_print: (ptr, len) => {
      const name = envName(ptr, len, "env.print");
      emit(`${ENV_PREFIX}${name}=${envValue(name)}`);
    },
    process_exit: (code) => exitCode(code),
    process_exit_f64: (code) => exitCode(code),
    number_remainder: (a, b) => a % b,
    number_power: (a, b) => a ** b,
    number_random: () => Math.random(),
    string_char_at16: (ptr, len, index) => writeResult(decodeUtf16(guestRange(ptr, len, C.maxStringBytes, invalid), invalid).charAt(index)),
    string_char_code_at16: (ptr, len, index) => decodeUtf16(guestRange(ptr, len, C.maxStringBytes, invalid), invalid).charCodeAt(index),
    ...schedulerImports(ctx, exec, policies),
  };
  impl.env_has16 = impl.env_has;
  impl.env_get16 = impl.env_get;
  impl.env_print16 = impl.env_print;
  for (const name of MATH_FUNCTIONS) impl[`number_${name}`] = Math[name];

  const fallback = (op) => {
    if (op.results.length === 0) return undefined;
    if (op.name.startsWith("net_fetch")) return -1;
    return op.results[0] === "i64" ? 0n : 0;
  };

  // embedding.md "Call protocol": budget, reset, validate, authorize, effect, complete.
  function wrap(op) {
    const body = impl[op.name];
    return (...args) => {
      if (!op.scheduler && !TIMER_BOOKKEEPING.has(op.name)) {
        exec.hostCalls += 1;
        if (exec.hostCalls > C.maxHostCalls) throw new ExecutionFailure("host call budget exhausted");
      }
      if (now() > exec.deadline) throw new ExecutionFailure("execution deadline exceeded");
      if (op.capability) {
        setGlobal("env_result_len", 0);
        setGlobal("result_status", C.status.Ok);
      }
      try {
        return body(...args);
      } catch (error) {
        if (!(error instanceof Recoverable)) throw error;
        setGlobal("result_status", error.status);
        setGlobal("env_result_len", 0);
        emit(LOG_PREFIX + error.diagnostic);
        return fallback(op);
      }
    };
  }

  const imports = { varta: Object.fromEntries(info.used.map((op) => [op.name, wrap(op)])) };
  return { exec, imports };
}
