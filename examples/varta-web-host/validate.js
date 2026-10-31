// embedding.md "Module validation": everything checked before instantiation,
// including the tool catalog. Function and frame catalogs are in catalog.js.
import { readModule, functionType, globalAt, WasmFormatError } from "./wasm-binary.js";
import { exactKeys } from "./strict-json.js";
import { ValidationError } from "./errors.js";
import {
  SECTION, LEGACY_PROFILE, LEGACY_HOST_ABI, TOOL_NAME, LIMIT, hostLimit, sameSignature, utf8, customSections, strictSection,
} from "./context.js";
import { checkFunctionCatalog, checkFrameAdmission } from "./catalog.js";
import { IMPLEMENTED } from "./execution.js";

// Step 6: imports that do not require `memory`.
const MEMORY_FREE_IMPORTS = new Set([
  "log_i32", "log_f64", "log_f64_v2", "log_undefined", "number_remainder", "process_exit", "process_exit_f64", "time_now",
]);
const RESERVED_PREFIXES = ["varta.exception.", "varta.tool.", "varta.http_abi.", "varta.view_abi."];
// Profiles that admit the view protocol (`view_commit` and the event export).
export const VIEW_PROFILES = new Set(["checked-f64-utf16-v38"]);
const LANGUAGE_SCHEMA_VERSION = 1;
const VERSION_SECTION_BYTES = 4;
const PAGE_COUNT_BYTES = 4;
// Profiles whose modules may declare `varta.static` / `varta.intrinsics` (embedding.md step 3).
const STATIC_REGION_PROFILES = new Set(["checked-f64-utf16-v38"]);
const STATIC_DATA_REGION = "Static data (literals, descriptors, schemas)";
const PARAMETER_NAME = /^[A-Za-z_$][A-Za-z0-9_$]*$/;
const PHYSICAL_TYPE = { integer: "i32", boolean: "i32", string: "i64" };

/** Returns `validate(bytes)` for one host context. */
export function createValidator(ctx) {
  const { C, profiles, hostAbis, operations, capabilityIds, acceptanceOf } = ctx;
  const layout = ctx.contract.layout;
  const firstPageEnd = layout[STATIC_DATA_REGION].end;

  function readU32Section(module, name) {
    const sections = customSections(module, name);
    if (sections.length > 1) throw new ValidationError(`duplicate ${name} section`);
    if (sections.length === 0) return null;
    if (sections[0].data.length !== VERSION_SECTION_BYTES) throw new ValidationError(`${name} must hold exactly four bytes`);
    return new DataView(sections[0].data.buffer, sections[0].data.byteOffset, VERSION_SECTION_BYTES).getUint32(0, true);
  }

  function resolveProfile(module) {
    const hostAbi = readU32Section(module, SECTION.abi);
    if (hostAbi !== null && !hostAbis.has(hostAbi)) throw new ValidationError(`unknown host ABI ${hostAbi}`);
    const languages = customSections(module, SECTION.language);
    if (languages.length > 1) throw new ValidationError("duplicate varta.language section");
    if (languages.length === 0) {
      if (hostAbi === null) return { profile: null, hostAbi: null };
      if (hostAbi === LEGACY_HOST_ABI) return { profile: profiles.get(LEGACY_PROFILE), hostAbi };
      throw new ValidationError("host ABI above 1 without varta.language");
    }
    if (languages[0].data.length > C.maxMetadataBytes) throw new ValidationError("varta.language too large");
    const meta = strictSection(languages[0].data, "varta.language");
    try {
      exactKeys(meta, ["schemaVersion", "profile"], [], "varta.language");
    } catch (error) {
      throw new ValidationError(error.message);
    }
    if (meta.schemaVersion?.raw !== String(LANGUAGE_SCHEMA_VERSION)) throw new ValidationError("unsupported varta.language schemaVersion");
    const profile = profiles.get(meta.profile);
    if (!profile) throw new ValidationError(`unknown profile ${meta.profile}`);
    if (hostAbi !== profile.hostAbi) throw new ValidationError(`profile ${profile.name} requires host ABI ${profile.hostAbi}`);
    return { profile, hostAbi };
  }

  function checkStructure(module, profile) {
    if (module.types.length > C.maxSignatures) throw new ValidationError("too many function types");
    for (const type of module.types) {
      if (type.params.length > C.maxParameters || type.results.length > C.maxResults) throw new ValidationError("function type too wide");
    }
    if (module.imports.length > C.maxImports) throw new ValidationError("too many imports");
    if (module.memories.length > C.maxMemories) throw new ValidationError("more than one memory");
    for (const memory of module.memories) {
      if (memory.is64 || memory.shared) throw new ValidationError("memory must be 32-bit and not shared");
      if (memory.min * C.pageBytes > C.maxMemoryBytes) throw new ValidationError("initial memory too large");
      // Step 4: JavaScript hosts have no store limiter, so the declared maximum is the budget.
      if (memory.max === null || memory.max * C.pageBytes > C.maxMemoryBytes) throw new ValidationError("memory maximum absent or above the memory budget");
    }
    if (module.tables.length > C.maxTables) throw new ValidationError("more than one table");
    if (module.tables.some((t) => t.limits.min > C.maxTableElements)) throw new ValidationError("table too large");
    if (module.globals.length > C.maxGlobals) throw new ValidationError("too many globals");
    if (module.bodySizes.some((size) => size > C.maxBodyBytes)) throw new ValidationError("function body too large");
    const acceptance = acceptanceOf(profile);
    const imported = module.imports.filter((i) => i.kind === "function").length;
    const counted = module.functionTypes.length + (acceptance.functionLimitCountsImports ? imported : 0);
    if (counted > acceptance.maxDefinedFunctions) throw new ValidationError("too many defined functions");
    checkStaticRegions(module, profile);
  }

  /** One page count section (`varta.static`, `varta.intrinsics`): a little-endian u32 of at least 1. */
  function pageCount(module, name) {
    const sections = customSections(module, name);
    if (sections.length === 0) return 0;
    if (sections.length > 1) throw new ValidationError(`duplicate ${name} section`);
    const data = sections[0].data;
    if (data.length !== PAGE_COUNT_BYTES) throw new ValidationError(`${name} must be a u32 page count`);
    const pages = new DataView(data.buffer, data.byteOffset, data.length).getUint32(0, true);
    if (pages === 0) throw new ValidationError(`${name} declares zero pages`);
    return pages;
  }

  /**
   * v38 static regions (embedding.md "Module validation", step 3): the static
   * extension and the intrinsic region after it, both within the initial memory,
   * and every data segment active in memory 0 at an i32.const offset, wholly in
   * the first page, the extension or the intrinsic region.
   */
  function checkStaticRegions(module, profile) {
    const extensionPages = pageCount(module, SECTION.staticExtension);
    const intrinsicPages = pageCount(module, SECTION.intrinsics);
    if (profile === null || !STATIC_REGION_PROFILES.has(profile.name)) {
      if (extensionPages || intrinsicPages) throw new ValidationError("static regions need checked-f64-utf16-v38");
      return;
    }
    const { base, pageBytes, maxPages } = layout.staticExtension;
    if (extensionPages > maxPages || extensionPages + intrinsicPages > layout.intrinsicRegion.maxTotalPages) throw new ValidationError("static regions exceed their page limit");
    const regionsEnd = base + (extensionPages + intrinsicPages) * pageBytes;
    const initial = module.memories[0]?.min ?? 0;
    if (regionsEnd > base && regionsEnd > initial * pageBytes) throw new ValidationError("static regions lie outside the initial memory");
    const regions = [
      [0, firstPageEnd],
      [base, base + extensionPages * pageBytes],
      [base + extensionPages * pageBytes, regionsEnd],
    ];
    for (const segment of module.data) {
      if (!segment.active || segment.memory !== 0 || segment.offset === null) throw new ValidationError("v38 data segments must be active in memory 0 at an i32.const offset");
      const start = segment.offset >>> 0;
      const end = start + segment.size;
      if (!regions.some(([from, to]) => start >= from && end <= to)) throw new ValidationError("data segment outside the first page and the declared static regions");
    }
  }

  function checkImports(module, profile, hostAbi) {
    const used = [];
    for (const entry of module.imports) {
      if (entry.module !== "varta" || entry.kind !== "function") throw new ValidationError(`import ${entry.module}.${entry.name} is not a varta function`);
      const op = operations.get(entry.name);
      if (!op) throw new ValidationError(`unknown host operation ${entry.name}`);
      if (!sameSignature(module.types[entry.type], op)) throw new ValidationError(`import ${entry.name} has the wrong signature`);
      if (profile === null) {
        if (op.minHostAbi !== LEGACY_HOST_ABI) throw new ValidationError(`generic Wasm may not import ${entry.name}`);
      } else {
        if (op.minHostAbi > hostAbi) throw new ValidationError(`${entry.name} needs host ABI ${op.minHostAbi}`);
        if (!op.admittedUnder.includes(profile.name)) throw new ValidationError(`${entry.name} is not admitted under ${profile.name}`);
      }
      if (!IMPLEMENTED.has(entry.name)) throw new ValidationError(`this host does not implement ${entry.name}; refusing the module before instantiation`);
      used.push(op);
    }
    return used;
  }

  function exportedGlobal(module, exportsByName, name) {
    const entry = exportsByName.get(name);
    if (!entry) return null;
    if (entry.kind !== "global") throw new ValidationError(`${name} must be a global`);
    return globalAt(module, entry.index);
  }

  function exportedFunctionType(module, exportsByName, name) {
    const entry = exportsByName.get(name);
    if (!entry) return null;
    if (entry.kind !== "function") throw new ValidationError(`${name} must be a function`);
    return functionType(module, entry.index);
  }

  function checkExports(module, profile, used, catalog, exportsByName) {
    const needsMemory = used.some((op) => !MEMORY_FREE_IMPORTS.has(op.name));
    const memory = exportsByName.get("memory");
    if (needsMemory && !memory) throw new ValidationError("module must export memory");
    if (memory && memory.kind !== "memory") throw new ValidationError("memory export must be a memory");

    const needsGlobals = used.some((op) => op.resultGlobals);
    for (const name of ["env_result_len", "result_status"]) {
      const global = exportedGlobal(module, exportsByName, name);
      if (needsGlobals && !global) throw new ValidationError(`module must export ${name}`);
      if (global && (global.valueType !== "i32" || !global.mutable)) throw new ValidationError(`${name} must be a mutable i32 global`);
    }
    const start = exportedFunctionType(module, exportsByName, "_start");
    if (exportsByName.has("_start") && !sameSignature(start, { params: [], results: [] })) throw new ValidationError("_start must be () -> ()");
    for (const [name, params, results] of [["varta.value_abi.allocate", ["i32"], ["i32"]], ["varta.value_abi.retain", ["i64"], ["i64"]]]) {
      if (exportsByName.has(name) && !sameSignature(exportedFunctionType(module, exportsByName, name), { params, results })) throw new ValidationError(`${name} has the wrong signature`);
    }

    const exceptionNames = new Set();
    if (profile?.exceptionChannel) {
      const pending = exportedGlobal(module, exportsByName, "varta.exception.pending");
      const value = exportedGlobal(module, exportsByName, "varta.exception.value");
      if (!pending || pending.valueType !== "i32" || !pending.mutable) throw new ValidationError("varta.exception.pending must be a mutable i32 global");
      if (!value || value.valueType !== "i64" || !value.mutable) throw new ValidationError("varta.exception.value must be a mutable i64 global");
      exceptionNames.add("varta.exception.pending").add("varta.exception.value");
      if (profile.exceptionVersion) {
        const { global: name, value: expected } = profile.exceptionVersion;
        const version = exportedGlobal(module, exportsByName, name);
        if (!version || version.valueType !== "i32" || version.mutable || version.init !== expected) {
          throw new ValidationError(`${name} must be an immutable i32 global equal to ${expected}`);
        }
        exceptionNames.add(name);
      }
    }
    // The view event export: v38 only, with the registry signature (embedding.md "Event delivery").
    const viewEvent = `varta.${C.view.event.name}`;
    if (exportsByName.has(viewEvent)) {
      if (profile === null || !VIEW_PROFILES.has(profile.name)) throw new ValidationError(`${viewEvent} is admitted only under the view profiles`);
      if (!sameSignature(exportedFunctionType(module, exportsByName, viewEvent), C.view.event)) throw new ValidationError(`${viewEvent} has the wrong signature`);
    }
    const toolExports = new Set((catalog?.tools ?? []).map((t) => t.export));
    for (const entry of module.exports) {
      const prefix = RESERVED_PREFIXES.find((p) => entry.name.startsWith(p));
      if (!prefix) continue;
      if (prefix === "varta.exception." && exceptionNames.has(entry.name)) continue;
      if (prefix === "varta.tool." && toolExports.has(entry.name)) continue;
      if (entry.name === viewEvent) continue;
      throw new ValidationError(`unlisted reserved export ${entry.name}`);
    }
  }

  function checkValueSection(module, profile, used) {
    const valueAbi = readU32Section(module, SECTION.values);
    const required = profile?.exceptionChannel || used.some((op) => /^(json|union|value)_/.test(op.name));
    if (required && valueAbi === null) throw new ValidationError("module requires a varta.values section");
    if (valueAbi !== null && !acceptanceOf(profile).values.includes(valueAbi)) {
      throw new ValidationError(`varta.values ${valueAbi} is not accepted for ${profile?.name ?? "generic Wasm"}`);
    }
  }

  function checkToolCatalog(module, profile, exportsByName) {
    const sections = customSections(module, SECTION.tools);
    if (sections.length === 0) return null;
    if (sections.length > 1) throw new ValidationError("duplicate varta.tools section");
    if (!profile?.toolHttp) throw new ValidationError("tool catalog outside a tool profile");
    if (customSections(module, SECTION.abi).length !== 1) throw new ValidationError("tool modules need exactly one varta.abi section");
    if (module.start !== null) throw new ValidationError("tool modules must not have a start section");
    if (sections[0].data.length > C.maxCatalogBytes) throw new ValidationError("tool catalog too large");
    const catalog = strictSection(sections[0].data, "tool catalog");
    try {
      exactKeys(catalog, ["contractVersion", "invocationAbiVersion", "tools"], [], "tool catalog");
    } catch (error) {
      throw new ValidationError(error.message);
    }
    if (catalog.contractVersion?.raw !== "1" || catalog.invocationAbiVersion?.raw !== "1") throw new ValidationError("unsupported tool catalog version");
    const tools = catalog.tools;
    if (!Array.isArray(tools) || tools.length === 0 || tools.length > C.maxTools) throw new ValidationError("tool count out of range");
    let anyString = false;
    tools.forEach((tool, index) => {
      try {
        exactKeys(tool, ["name", "description", "export", "parameters", "result", "requiredEffects", "limits"], [], "tool");
      } catch (error) {
        throw new ValidationError(error.message);
      }
      if (typeof tool.name !== "string" || !TOOL_NAME.test(tool.name) || utf8.encode(tool.name).length > C.maxToolNameBytes) throw new ValidationError("bad tool name");
      if (index > 0 && !(tools[index - 1].name < tool.name)) throw new ValidationError("tools must be sorted by name without duplicates");
      if (tool.export !== `varta.tool.${tool.name}`) throw new ValidationError(`bad export for ${tool.name}`);
      if (typeof tool.description !== "string" || tool.description.trim() === "" || tool.description.includes("\0") || utf8.encode(tool.description).length > C.maxToolDescriptionBytes) {
        throw new ValidationError(`bad description for ${tool.name}`);
      }
      if (!Array.isArray(tool.parameters) || tool.parameters.length > C.maxToolParameters) throw new ValidationError(`bad parameters for ${tool.name}`);
      const names = new Set();
      for (const parameter of tool.parameters) {
        try {
          exactKeys(parameter, ["name", "valueType"], [], "parameter");
        } catch (error) {
          throw new ValidationError(error.message);
        }
        if (typeof parameter.name !== "string" || !PARAMETER_NAME.test(parameter.name) || utf8.encode(parameter.name).length > C.maxToolNameBytes || names.has(parameter.name)) {
          throw new ValidationError(`bad parameter name in ${tool.name}`);
        }
        names.add(parameter.name);
        if (!(parameter.valueType in PHYSICAL_TYPE)) throw new ValidationError(`bad parameter type in ${tool.name}`);
      }
      if (!(tool.result in PHYSICAL_TYPE) && tool.result !== "void") throw new ValidationError(`bad result type in ${tool.name}`);
      const effects = tool.requiredEffects;
      if (!Array.isArray(effects) || effects.some((e, i) => typeof e !== "string" || !capabilityIds.has(e) || (i > 0 && !(effects[i - 1] < e)))) {
        throw new ValidationError(`bad requiredEffects in ${tool.name}`);
      }
      try {
        exactKeys(tool.limits, Object.keys(C.toolLimits), [], "limits");
      } catch (error) {
        throw new ValidationError(error.message);
      }
      for (const [key, expected] of Object.entries(C.toolLimits)) {
        if (tool.limits[key]?.raw !== String(expected)) throw new ValidationError(`tool ${tool.name} limit ${key} differs from the registry`);
      }
      const physical = {
        params: tool.parameters.map((p) => PHYSICAL_TYPE[p.valueType]),
        results: tool.result === "void" ? [] : [PHYSICAL_TYPE[tool.result]],
      };
      const entry = exportsByName.get(tool.export);
      if (!entry || entry.kind !== "function" || !sameSignature(functionType(module, entry.index), physical)) throw new ValidationError(`${tool.export} has the wrong signature`);
      anyString ||= tool.result === "string" || tool.parameters.some((p) => p.valueType === "string");
    });
    if (anyString) {
      for (const [name, params, results] of [["varta.tool_abi.copy_string", ["i32", "i32"], ["i64"]], ["varta.tool_abi.retain_string", ["i64"], ["i64"]]]) {
        if (!sameSignature(exportedFunctionType(module, exportsByName, name), { params, results })) throw new ValidationError(`string tools need ${name}`);
      }
    }
    // requiredEffects is not recomputed from the call graph (a MAY); it is
    // reported as unverified metadata and never treated as a grant.
    return catalog;
  }

  /**
   * Validates a module before instantiation (embedding.md "Module validation").
   * @param {Uint8Array} bytes
   */
  return function validate(bytes) {
    if (bytes.length > C.maxWasmBytes) throw new ValidationError("Wasm file too large");
    let module;
    try {
      module = readModule(bytes);
    } catch (error) {
      if (error instanceof WasmFormatError) throw new ValidationError(error.message);
      throw error;
    }
    const { profile, hostAbi } = resolveProfile(module);
    if (bytes.length > hostLimit(ctx.contract, LIMIT.wasmBytes, profile?.name ?? null)) throw new ValidationError("Wasm file too large for its profile");
    checkStructure(module, profile);
    const used = checkImports(module, profile, hostAbi);
    const exportsByName = new Map(module.exports.map((e) => [e.name, e]));
    const catalog = checkToolCatalog(module, profile, exportsByName);
    checkExports(module, profile, used, catalog, exportsByName);
    checkValueSection(module, profile, used);
    if (module.start !== null && (profile?.exceptionChannel || catalog)) throw new ValidationError("start section not allowed in this module");
    if (customSections(module, SECTION.http).length || used.some((op) => op.name === "http_service")) {
      throw new ValidationError("HTTP service modules need level H1, which this host does not implement");
    }
    const functionCatalog = checkFunctionCatalog(ctx, module, profile);
    const hasFrames = checkFrameAdmission(module, profile, functionCatalog, exportsByName);
    if (used.some((op) => op.scheduler) && !hasFrames) throw new ValidationError("scheduler imports require frame layouts and varta_resume");
    return { module, profile, hostAbi, used, catalog, exportsByName, hasFrames };
  };
}
