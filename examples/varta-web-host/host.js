// Browser/Node host for Varta portable Wasm, written only from docs/embedding.md.
// Levels: T1 (tool host), R1 (run host), R2 (scheduled run host). Not H1.
// Every number comes from contract.json, which tools/extract-contract.mjs builds
// from the generated tables of the published contract.
//
// Honest guarantees (embedding.md "Budget guarantees", "Function and frame
// catalogs"): memory through the declared maximum; host calls and output counted;
// the deadline is checked at host calls only; fuel is NOT enforced; reduced
// catalog validation (normative minimum and frame admission only; guest memory is
// never read through the catalog).
//
// This file is the lifecycle: policies, run, invoke. Validation, catalogs,
// execution, the scheduler and the Tool ABI live in their own modules.
import { parseManifest, checkRootNameGrammar, PolicyError } from "./policy.js";
import { ValidationError, ExecutionFailure, GuestExit } from "./errors.js";
import { createContext, LEGACY_HOST_ABI, TOOL_NAME, utf8 } from "./context.js";
import { createValidator } from "./validate.js";
import { createExecution } from "./execution.js";
import { checkException } from "./exception.js";
import { eventLoop } from "./scheduler.js";
import { toolFailure, parseToolArguments, transferString, toolResult } from "./tool-abi.js";

export { ValidationError, ExecutionFailure } from "./errors.js";
export { formatLegacyF64 } from "./execution.js";

export function createHost(contract) {
  const ctx = createContext(contract);
  const { C, hostAbis } = ctx;
  const validate = createValidator(ctx);
  const manifestOptions = { hostAbis, grantKinds: contract.grantKinds, budgetTable: contract.manifestBudgets, limits: contract.manifestLimits };
  checkRootNameGrammar(contract.manifestLimits, contract.rootNameExamples);

  function parsePolicies(options) {
    try {
      return {
        package: parseManifest(options.manifest ?? null, { where: "manifest", ...manifestOptions }),
        // Same schema; its abiVersion is not compared with the module's.
        ceiling: options.ceiling == null ? null : parseManifest(options.ceiling, { where: "host ceiling", ceiling: true, ...manifestOptions }),
      };
    } catch (error) {
      if (error instanceof PolicyError) throw new ValidationError(error.message);
      throw error;
    }
  }

  function checkManifestAbi(policies, info) {
    const declared = policies.package.abiVersion;
    const module = info.hostAbi ?? LEGACY_HOST_ABI;
    if (declared !== undefined && declared !== module) throw new ValidationError(`manifest abiVersion ${declared} does not match the module's host ABI ${module}`);
  }

  function classify(error) {
    if (error instanceof GuestExit) return { kind: "exit", code: error.code };
    if (error instanceof ExecutionFailure) return { kind: "failure", message: error.message };
    if (error instanceof WebAssembly.RuntimeError || error instanceof RangeError) return { kind: "failure", message: `trap: ${error.message}` };
    throw error;
  }

  /**
   * Level R1/R2: one execution of `_start` plus checks and the event loop
   * (microtask checkpoint and timers).
   * `logs` hold the reference line texts.
   * @returns {Promise<{status: "ok" | "failed" | "exit", logs: string[], exitCode?: number, error?: string}>}
   */
  async function run(bytes, options = {}) {
    const policies = parsePolicies(options);
    const info = validate(bytes);
    checkManifestAbi(policies, info);
    const { exec, imports } = createExecution(ctx, info, policies, options);
    // A JavaScript host caps memory only through the module's declared maximum.
    const declared = (info.module.memories[0]?.max ?? 0) * C.pageBytes;
    if (declared > exec.budgets.memoryBytes) throw new ValidationError(`this host cannot cap memory below the module's declared maximum (${declared} bytes > memoryBytes ${exec.budgets.memoryBytes})`);
    try {
      exec.instance = await WebAssembly.instantiate(await WebAssembly.compile(bytes), imports);
      checkException(C, exec, info, "initial");
      exec.instance.exports._start?.();
      checkException(C, exec, info, "_start");
      await eventLoop(ctx, exec, info);
      return { status: "ok", logs: exec.logs };
    } catch (error) {
      const outcome = classify(error);
      if (outcome.kind === "exit") return { status: "exit", exitCode: outcome.code, logs: exec.logs };
      return { status: "failed", error: outcome.message, logs: exec.logs };
    }
  }

  /**
   * Level T1: one published tool in a fresh instance; never runs `_start`.
   * Returns the development CLI's `varta invoke --json` envelope.
   */
  async function invoke(bytes, name, argumentsText = "{}", options = {}) {
    if (!TOOL_NAME.test(name) || utf8.encode(name).length > C.maxToolNameBytes) return toolFailure("unknown_tool", `invalid tool name: ${name}`);
    let policies;
    try {
      policies = parsePolicies(options);
    } catch (error) {
      return toolFailure("invalid_policy", error.message);
    }
    let info;
    try {
      info = validate(bytes);
      checkManifestAbi(policies, info);
    } catch (error) {
      if (error instanceof ValidationError) return toolFailure("invalid_artifact", error.message);
      throw error;
    }
    const tool = info.catalog?.tools.find((t) => t.name === name);
    if (!tool) return toolFailure("unknown_tool", `unknown published tool: ${name}`);
    if (utf8.encode(argumentsText).length > Number(tool.limits.inputBytes.raw)) return toolFailure("invalid_arguments", "arguments too large");
    const parsed = parseToolArguments(tool, argumentsText);
    if (parsed.error) return toolFailure("invalid_arguments", `invalid tool arguments: ${parsed.error}`);

    const { exec, imports } = createExecution(ctx, info, policies, { ...options, deadlineMs: Number(tool.limits.deadlineMs.raw) });
    const stringLimit = Number(tool.limits.stringBytes.raw);
    let result;
    try {
      exec.instance = await WebAssembly.instantiate(await WebAssembly.compile(bytes), imports);
      const physical = tool.parameters.map((parameter, i) => (parameter.valueType === "string" ? transferString(C, exec, parsed.values[i], stringLimit) : parsed.values[i]));
      result = toolResult(exec, tool, exec.instance.exports[tool.export](...physical), stringLimit);
    } catch (error) {
      const outcome = classify(error);
      if (outcome.kind === "exit") return toolFailure("guest_exit", `guest exited with code ${outcome.code}`);
      return toolFailure("execution_failed", outcome.message);
    }
    if (result.error) return toolFailure("invalid_result", result.error);
    if (utf8.encode(JSON.stringify({ value: result.value })).length > Number(tool.limits.outputBytes.raw)) return toolFailure("invalid_result", "result exceeds outputBytes");
    return { logs: exec.logs, result: { value: result.value }, status: "ok" };
  }

  /**
   * Contract checks only: the scheduler and timer imports of one fresh execution,
   * callable directly, so a misuse can be raised without a crafted module. Throws
   * what the import throws (fatal scheduler errors carry their reference text).
   */
  function debugScheduler(manifest = null) {
    const used = contract.operations.filter((op) => op.scheduler || op.name.startsWith("timer_"));
    // The misuse table is checked under the last profile that admits `timer_start`.
    const timerStart = contract.operations.find((op) => op.name === "timer_start");
    const profile = ctx.profiles.get(timerStart.admittedUnder[timerStart.admittedUnder.length - 1]);
    const { exec, imports } = createExecution(ctx, { used, profile }, parsePolicies({ manifest }), {});
    exec.instance = { exports: {} };
    return { call: (name, ...args) => imports.varta[name](...args), exec };
  }

  return {
    validate,
    run,
    invoke,
    debugScheduler,
    guarantees: {
      memory: "module-declared maximum, checked before instantiation",
      fuel: "not enforced",
      deadline: "checked at host calls only; terminate the Worker for runaway guest loops",
      hostCalls: "counted",
      output: "counted on the reference line text",
      catalog: "reduced catalog validation: normative minimum and frame admission; guest memory is never read through the catalog",
      requiredEffects: "unverified metadata",
    },
  };
}
