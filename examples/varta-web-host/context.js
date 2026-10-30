// The host context: every number from contract.json, resolved once per host, and
// the small helpers the validation and execution modules share.
import { parseStrictJson } from "./strict-json.js";
import { ValidationError, ExecutionFailure } from "./errors.js";

export const SECTION = {
  abi: "varta.abi", language: "varta.language", values: "varta.values",
  tools: "varta.tools", http: "varta.http", functions: "varta.functions",
  staticExtension: "varta.static", intrinsics: "varta.intrinsics",
};
export const LEGACY_PROFILE = "checked-i32-utf8-v1";
export const LEGACY_HOST_ABI = 1;
export const TOOL_NAME = /^[A-Za-z][A-Za-z0-9_.-]*$/;
export const I32_MIN = -(2 ** 31);
export const I32_MAX = 2 ** 31 - 1;
export const MS_PER_SECOND = 1000;

export const sameTypes = (a, b) => a.length === b.length && a.every((t, i) => t === b[i]);
export const sameSignature = (type, sig) => Boolean(type) && sameTypes(type.params, sig.params) && sameTypes(type.results, sig.results);
export const utf8 = new TextEncoder();
export const strictUtf8 = new TextDecoder("utf-8", { fatal: true });
export const now = () => (typeof performance === "object" ? performance.now() : Date.now());

export const customSections = (module, name) => module.custom.filter((s) => s.name === name);

export function strictSection(data, where) {
  try {
    return parseStrictJson(strictUtf8.decode(data));
  } catch (error) {
    throw new ValidationError(`invalid ${where}: ${error.message}`);
  }
}

function limitBySource(contract, constant, position = 0) {
  for (const table of [contract.limits, contract.entryLimits]) {
    for (const row of Object.values(table)) {
      if (row.source.includes(constant)) return row.values[position];
    }
  }
  throw new Error(`contract has no limit ${constant}`);
}

function constants(contract) {
  const range = (name) => contract.layout[name];
  const status = Object.fromEntries(contract.codes.filter((c) => c.channel === "result_status").map((c) => [c.name, c.code]));
  const resume = Object.fromEntries(contract.codes.filter((c) => c.channel.startsWith("varta_resume")).map((c) => [`${c.channel.split(" ")[1]}.${c.name}`, c.code]));
  return {
    resultBase: range("Reusable host result buffer").start,
    maxResultBytes: range("Reusable host result buffer").end - range("Reusable host result buffer").start,
    exceptionRoot: range("Exception root slot (start of the root stack)").start,
    pageBytes: range("Wasm page").bytes,
    maxInputBytes: limitBySource(contract, "abi::MAX_INPUT_BYTES"),
    maxStringBytes: range("One host result / one string value").bytes,
    maxMemoryBytes: limitBySource(contract, "abi::MAX_MEMORY_BYTES"),
    maxWasmBytes: limitBySource(contract, "abi::MAX_WASM_BYTES"),
    maxBodyBytes: limitBySource(contract, "abi::MAX_WASM_FUNCTION_BODY_BYTES"),
    maxSignatures: limitBySource(contract, "abi::MAX_WASM_SIGNATURES"),
    maxParameters: limitBySource(contract, "abi::MAX_WASM_PARAMETERS"),
    maxUserFunctions: limitBySource(contract, "abi::MAX_USER_FUNCTIONS"),
    maxCatalogTypes: limitBySource(contract, "function::MAX_CAPTURE_TYPES", 0),
    maxCatalogBindings: limitBySource(contract, "function::MAX_CAPTURE_TYPES", 1),
    signatureBound: limitBySource(contract, "function::MAX_TYPES"),
    maxCaptures: limitBySource(contract, "function::MAX_CAPTURES"),
    maxFrameBindings: limitBySource(contract, "frame::MAX_FRAME_BINDINGS", 0),
    maxSuspensionStates: limitBySource(contract, "frame::MAX_FRAME_BINDINGS", 1),
    timer: timerConstants(contract),
    maxHostCalls: limitBySource(contract, "abi::MAX_HOST_CALLS"),
    maxOutputBytes: limitBySource(contract, "abi::MAX_OUTPUT_BYTES"),
    deadlineMs: limitBySource(contract, "abi::DEADLINE_SECONDS") * MS_PER_SECOND,
    maxMetadataBytes: limitBySource(contract, "language::MAX_METADATA_BYTES"),
    maxHeldTokens: limitBySource(contract, "scheduler::MAX_PENDING_CONTINUATIONS"),
    maxJobs: limitBySource(contract, "scheduler::MAX_EXECUTION_JOBS"),
    maxSerials: limitBySource(contract, "scheduler::MAX_PROMISE_SERIALS"),
    maxCatalogBytes: limitBySource(contract, "MAX_TOOL_CATALOG_BYTES"),
    maxTools: limitBySource(contract, "MAX_TOOLS", 0),
    maxToolParameters: limitBySource(contract, "MAX_TOOLS", 1),
    maxToolNameBytes: limitBySource(contract, "MAX_TOOL_NAME_BYTES", 0),
    maxToolDescriptionBytes: limitBySource(contract, "MAX_TOOL_NAME_BYTES", 1),
    toolLimits: contract.toolCatalogExample.tools[0].limits,
    status,
    resume,
  };
}

function timerConstants(contract) {
  const t = contract.timers;
  const [minDelay, maxDelay] = t["scheduler::TIMER_MIN_DELAY_MS, scheduler::TIMER_MAX_DELAY_MS"].split(" to ").map(Number);
  return {
    capability: /`([^`]+)`/.exec(t["scheduler::TIMER_CAPABILITY"])[1],
    interval: Number(t["scheduler::TIMER_INTERVAL"]),
    unref: Number(t["scheduler::TIMER_UNREF"]),
    minDelay,
    maxDelay,
    maxPending: Number(t["scheduler::MAX_PENDING_TIMERS"]),
  };
}

/** Resolves the contract tables one host needs. */
export function createContext(contract) {
  return {
    contract,
    C: constants(contract),
    // embedding.md "Scheduler errors": reference diagnostics, placeholders filled.
    schedulerError: (name, values = {}) =>
      new ExecutionFailure(contract.schedulerErrors[name].replace(/<([a-z_]+)>/g, (whole, key) => (key in values ? String(values[key]) : whole))),
    profiles: new Map(contract.profiles.map((p) => [p.name, p])),
    hostAbis: new Set(contract.profiles.map((p) => p.hostAbi)),
    operations: new Map(contract.operations.map((o) => [o.name, o])),
    capabilityIds: new Set(contract.operations.map((o) => o.capability).filter(Boolean)),
    acceptanceOf: (profile) => contract.acceptance[profile?.name ?? LEGACY_PROFILE],
  };
}
