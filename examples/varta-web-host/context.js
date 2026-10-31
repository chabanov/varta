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

// Labels of the generated host limit tables (embedding.md "Module limits", "Scheduler limits").
export const LIMIT = {
  wasmBytes: "Wasm module (bytes)",
  metadataBytes: "`varta.language` metadata (bytes)",
  parameters: "Parameters of one function type",
  results: "Results of one function type",
  imports: "Imports",
  memories: "Memories",
  tables: "Tables",
  tableElements: "Initial elements of the table",
  globals: "Globals",
  heldTokens: "Tokens held at once (queued and registered)",
  pendingTimers: "Timers armed or awaiting their take at once",
  jobs: "Tokens taken from the queue per execution (v38: per turn)",
  serials: "Promise serials, and separately tracking ids and timer ids, per execution",
};

/** The value of a host limit for a profile (null: unspecified generic Wasm, the frozen row). */
export function hostLimit(contract, label, profileName) {
  const rows = contract.hostLimits.filter((r) => r.limit === label);
  const name = profileName ?? LEGACY_PROFILE;
  const row = rows.find((r) => r.profiles === null || r.profiles.includes(name));
  if (!row) throw new Error(`contract has no ${label} limit for ${name}`);
  return row.value;
}

/** The largest value of a host limit over every profile (bounds input before the profile is known). */
function largestLimit(contract, label) {
  return Math.max(...contract.hostLimits.filter((r) => r.limit === label).map((r) => r.value));
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
    maxWasmBytes: largestLimit(contract, LIMIT.wasmBytes),
    maxBodyBytes: limitBySource(contract, "abi::MAX_WASM_FUNCTION_BODY_BYTES"),
    maxSignatures: limitBySource(contract, "abi::MAX_WASM_SIGNATURES"),
    maxParameters: hostLimit(contract, LIMIT.parameters, null),
    maxResults: hostLimit(contract, LIMIT.results, null),
    maxImports: hostLimit(contract, LIMIT.imports, null),
    maxMemories: hostLimit(contract, LIMIT.memories, null),
    maxTables: hostLimit(contract, LIMIT.tables, null),
    maxTableElements: hostLimit(contract, LIMIT.tableElements, null),
    maxGlobals: hostLimit(contract, LIMIT.globals, null),
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
    maxMetadataBytes: hostLimit(contract, LIMIT.metadataBytes, null),
    maxJobs: hostLimit(contract, LIMIT.jobs, null),
    maxSerials: hostLimit(contract, LIMIT.serials, null),
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
