// embedding.md "Function and frame catalogs" (reduced validation: the normative
// minimum, rules 1-4) and "Frame admission (R2, normative)", rules 1-5. Guest
// memory is never read through the catalog.
import { functionType } from "./wasm-binary.js";
import { scanBody } from "./wasm-code.js";
import { exactKeys, JsonNumber } from "./strict-json.js";
import { ValidationError } from "./errors.js";
import { SECTION, customSections, strictSection, sameTypes, sameSignature } from "./context.js";

const CATALOG_FIELDS = ["version", "value_version", "types", "bindings", "functions"];
// embedding.md "Catalog field names": `states[].input` is always one of these JSON
// strings; `return_input` is one of them, JSON null or absent. No member takes "null".
const CATALOG_INPUT_KINDS = ["float64", "boolean", "undefined", "guest"];
const CATALOG_LIVENESS_VERSION = 3;
// embedding.md "Catalog field names": type kinds; index fields per kind are in rule 4.
const CATALOG_TYPE_KINDS = new Set([
  "i32", "float64", "boolean", "string", "null", "undefined", "bigInt", "error", "function", "record",
  "array", "dictionary", "primitiveUnion", "referenceUnion", "class", "generator", "unknown", "promise",
]);
const MIN_UNION_MEMBERS = 2;
const RESUME_SIGNATURE = { params: ["i64", "i32", "i64"], results: ["i32"] };
const RESUME_BODY_SIGNATURE = { params: ["i64", "i32", "i64"], results: [] };

/** Byte-wise comparison of the UTF-8 encodings of two strings. */
function compareUtf8(a, b) {
  const x = new TextEncoder().encode(a);
  const y = new TextEncoder().encode(b);
  for (let i = 0; i < Math.min(x.length, y.length); i += 1) if (x[i] !== y[i]) return x[i] - y[i];
  return x.length - y.length;
}

/** Normative minimum (rules 1-4); returns the frame layouts, or null without a catalog. */
export function checkFunctionCatalog(ctx, module, profile) {
  const { C, contract, acceptanceOf } = ctx;
  const sections = customSections(module, SECTION.functions);
  const accepted = profile ? acceptanceOf(profile).catalogs : [];
  if (sections.length === 0) return null;
  if (sections.length > 1) throw new ValidationError("duplicate varta.functions section");
  if (accepted.length === 0) throw new ValidationError(`${profile?.name ?? "generic Wasm"} must not carry varta.functions`);
  if (sections[0].data.length > C.maxWasmBytes) throw new ValidationError("varta.functions too large");
  const catalog = strictSection(sections[0].data, "varta.functions");
  const version = catalog?.version instanceof JsonNumber ? Number(catalog.version.raw) : NaN;
  const entry = accepted.find((a) => a.version === version);
  if (!entry) throw new ValidationError(`varta.functions version ${catalog?.version?.raw} is not accepted for ${profile.name}`);
  try {
    exactKeys(catalog, entry.frames ? [...CATALOG_FIELDS, "frames"] : CATALOG_FIELDS, [], "varta.functions");
  } catch (error) {
    throw new ValidationError(error.message);
  }
  if (catalog.value_version?.raw !== String(profile.valueAbi)) throw new ValidationError("varta.functions value_version differs from the profile's value ABI");
  for (const field of [...CATALOG_FIELDS.slice(2), ...(entry.frames ? ["frames"] : [])]) {
    if (!Array.isArray(catalog[field])) throw new ValidationError(`varta.functions ${field} must be an array`);
  }
  if (entry.frames && catalog.frames.length === 0) throw new ValidationError("frame catalogs must list at least one frame");
  checkCatalogShape(contract, catalog, version);
  checkCatalogIndices(C, catalog, entry.frames);
  return { frames: entry.frames ? catalog.frames : [] };
}

// embedding.md "Function and frame catalogs", optionality table: which members may
// be absent, which may be null, everything else required and non-null, and no
// unknown members. Version 3 (liveness layouts) requires `capacity`; version 2
// (cell layouts) forbids `capacity` and `saved`.
function checkCatalogShape(contract, catalog, version) {
  const fail = (what) => {
    throw new ValidationError(`varta.functions ${what}`);
  };
  const shape = (value, where, required, absent = [], nullable = []) => {
    if (value === null || typeof value !== "object" || Array.isArray(value) || value instanceof JsonNumber) fail(`${where} must be an object`);
    for (const key of Object.keys(value)) {
      if (!required.includes(key) && !absent.includes(key)) fail(`${where} has unknown member ${key}`);
      if (value[key] === null && !nullable.includes(key)) fail(`${where}.${key} must not be null`);
    }
    for (const key of required) if (!(key in value)) fail(`${where} is missing ${key}`);
    return value;
  };
  const arr = (value, where) => (Array.isArray(value) ? value : fail(`${where} must be an array`));
  const inputs = new Set(CATALOG_INPUT_KINDS);
  const input = (value, where, nullable) => {
    if (value === null && nullable) return;
    if (typeof value !== "string" || !inputs.has(value)) fail(`${where} must be one of ${CATALOG_INPUT_KINDS.join(", ")}`);
  };
  // Members per kind come from the generated catalog-types table; kinds without
  // members accept only `kind`.
  for (const type of catalog.types) {
    const spec = contract.catalogTypes[type?.kind];
    if (!spec) fail(`types[] has unknown kind ${type?.kind}`);
    const optional = spec.mayBeAbsent ? spec.members : [];
    shape(type, "types[]", ["kind", ...spec.members.filter((m) => !optional.includes(m))], optional, spec.nullable ? spec.members : []);
    if (type.kind === "record") {
      const fields = arr(type.fields, "types[].fields");
      fields.forEach((field, i) => {
        shape(field, "types[].fields[]", ["name", "ty", "optional"]);
        // Strictly ascending by UTF-8 bytes (round 9), which differs from UTF-16
        // string order for characters outside the BMP.
        if (typeof field.name !== "string" || (i > 0 && compareUtf8(fields[i - 1].name, field.name) >= 0)) fail("types[].fields must be strictly ascending by UTF-8 bytes of name");
      });
    }
  }
  for (const binding of catalog.bindings) shape(binding, "bindings[]", ["ty", "result"]);
  for (const fn of catalog.functions) shape(fn, "functions[]", ["code", "signature", "captures"]);
  for (const frame of catalog.frames ?? []) {
    shape(frame, "frames[]", ["code", "slots", "states", "exception"], ["capacity", "result", "return_input", "generator"], ["capacity", "result", "return_input"]);
    const liveness = version === CATALOG_LIVENESS_VERSION;
    if (liveness && (frame.capacity === undefined || frame.capacity === null)) fail("frames[].capacity is required in liveness layouts");
    if (!liveness && frame.capacity !== undefined && frame.capacity !== null) fail("frames[].capacity is not allowed in cell layouts");
    input(frame.return_input ?? null, "frames[].return_input", true);
    for (const slot of arr(frame.slots, "frames[].slots")) shape(slot, "frames[].slots[]", ["binding", "shared"]);
    for (const state of arr(frame.states, "frames[].states")) {
      shape(state, "frames[].states[]", ["input", "yielded"], ["saved"]);
      input(state.input, "frames[].states[].input", false);
      if (!liveness && state.saved !== undefined) fail("frames[].states[].saved is not allowed in cell layouts");
      // `saved` entries: the document names only `binding`; real catalogs also carry `shared`.
      for (const saved of arr(state.saved ?? [], "frames[].states[].saved")) shape(saved, "frames[].states[].saved[]", ["binding", "shared"]);
    }
  }
}

// embedding.md "Function and frame catalogs", rule 4: array bounds, exactly the
// listed index fields, and the stated ranges of the numeric non-index fields.
function checkCatalogIndices(C, catalog, framesVersion) {
  const fail = (what) => {
    throw new ValidationError(`varta.functions ${what}`);
  };
  const object = (value, where) => (value !== null && typeof value === "object" && !Array.isArray(value) && !(value instanceof JsonNumber) ? value : fail(`${where} must be an object`));
  const list = (value, where, max) => {
    if (!Array.isArray(value)) fail(`${where} must be an array`);
    if (value.length > max) fail(`${where} has more than ${max} entries`);
    return value;
  };
  const natural = (value) => (value instanceof JsonNumber && /^(0|[1-9][0-9]*)$/.test(value.raw) ? Number(value.raw) : -1);
  const index = (value, length, where) => {
    const n = natural(value);
    if (n < 0 || n >= length) fail(`${where} index out of range`);
    return n;
  };
  const below = (value, bound, where) => {
    const n = natural(value);
    if (n < 0 || n >= bound) fail(`${where} must be below ${bound}`);
    return n;
  };
  const types = list(catalog.types, "types", C.maxCatalogTypes);
  const bindings = list(catalog.bindings, "bindings", C.maxCatalogBindings);
  const functions = list(catalog.functions, "functions", C.maxUserFunctions);
  for (const binding of bindings) index(object(binding, "binding").ty, types.length, "bindings[].ty");
  for (const type of types.map((t) => object(t, "type"))) {
    if (!CATALOG_TYPE_KINDS.has(type.kind)) fail(`unknown type kind ${type.kind}`);
    if (type.kind === "record") for (const field of list(type.fields, "types[].fields", Infinity)) index(object(field, "field").ty, types.length, "types[].fields[].ty");
    if (type.kind === "array") index(type.element, types.length, "types[].element");
    if (type.kind === "dictionary") index(type.value, types.length, "types[].value");
    if (type.kind === "primitiveUnion" || type.kind === "referenceUnion") {
      const members = list(type.members, "types[].members", Infinity).map((member) => index(member, types.length, "types[].members[]"));
      if (members.length < MIN_UNION_MEMBERS || new Set(members).size !== members.length) fail("types[].members must hold at least two distinct entries");
    }
    if (type.kind === "function") below(type.signature, C.signatureBound, "types[].signature");
  }
  const codes = new Set();
  for (const fn of functions.map((f) => object(f, "function"))) {
    const code = below(fn.code, C.maxUserFunctions, "functions[].code");
    if (codes.has(code)) fail("functions[].code must be unique");
    codes.add(code);
    below(fn.signature, C.signatureBound, "functions[].signature");
    for (const capture of list(fn.captures, "functions[].captures", C.maxCaptures)) index(capture, bindings.length, "functions[].captures[]");
  }
  if (!framesVersion) return;
  const frameCodes = new Set();
  for (const frame of list(catalog.frames, "frames", C.maxUserFunctions).map((f) => object(f, "frame"))) {
    const code = below(frame.code, C.maxUserFunctions, "frames[].code");
    if (frameCodes.has(code)) fail("frames[].code must be unique");
    frameCodes.add(code);
    for (const slot of list(frame.slots, "frames[].slots", C.maxFrameBindings)) index(object(slot, "slot").binding, bindings.length, "frames[].slots[].binding");
    for (const state of list(frame.states, "frames[].states", C.maxSuspensionStates).map((st) => object(st, "state"))) {
      index(state.yielded, bindings.length, "frames[].states[].yielded");
      if (state.saved !== undefined) for (const saved of list(state.saved, "frames[].states[].saved", C.maxFrameBindings)) index(object(saved, "saved").binding, bindings.length, "frames[].states[].saved[].binding");
    }
    if (frame.result !== null && frame.result !== undefined) index(frame.result, bindings.length, "frames[].result");
    index(frame.exception, bindings.length, "frames[].exception");
    if (frame.capacity !== undefined && frame.capacity !== null && (natural(frame.capacity) < 0 || natural(frame.capacity) > C.maxFrameBindings)) fail(`frames[].capacity must be at most ${C.maxFrameBindings}`);
  }
}

/** Frame admission (R2, normative), rules 1-5; returns whether the module has frames. */
export function checkFrameAdmission(module, profile, functionCatalog, exportsByName) {
  const frames = functionCatalog?.frames ?? [];
  const resume = exportsByName.get("varta_resume");
  if (!profile?.frames || frames.length === 0) {
    if (resume) throw new ValidationError("varta_resume is exported without frame layouts");
    return false;
  }
  if (!resume) throw new ValidationError("frame layouts require the varta_resume export");
  const importedFunctions = module.imports.filter((i) => i.kind === "function").length;
  const definedCount = module.functionTypes.length;
  const isDefined = (index) => index >= importedFunctions && index < importedFunctions + definedCount;
  if (resume.kind !== "function" || !isDefined(resume.index)) throw new ValidationError("varta_resume must be a guest-defined function");
  if (!sameSignature(functionType(module, resume.index), RESUME_SIGNATURE)) throw new ValidationError("varta_resume has the wrong signature");
  let scan;
  try {
    scan = scanBody(module.bytes, module.bodies[resume.index - importedFunctions]);
  } catch (error) {
    throw new ValidationError(`cannot decode varta_resume: ${error.message}`);
  }
  if (scan.indirect || scan.table) throw new ValidationError("varta_resume must not use indirect calls or table instructions");
  if (scan.calls.some((target) => !isDefined(target) || target === resume.index)) throw new ValidationError("varta_resume may call only other guest-defined functions");
  const resumeBodies = new Set(scan.calls.filter((target) => sameSignature(functionType(module, target), RESUME_BODY_SIGNATURE)));
  if (resumeBodies.size !== frames.length) throw new ValidationError(`varta_resume has ${resumeBodies.size} resume bodies for ${frames.length} frame layouts`);
  if (module.exports.some((e) => e.kind === "function" && resumeBodies.has(e.index))) throw new ValidationError("resume bodies must not be exported");
  for (const frame of frames) {
    const code = frame?.code instanceof JsonNumber ? Number(frame.code.raw) : NaN;
    const creator = code + importedFunctions;
    if (!Number.isInteger(code) || !isDefined(creator) || creator === resume.index || resumeBodies.has(creator)) {
      throw new ValidationError(`frame layout code ${frame?.code?.raw} does not name a creator function`);
    }
    if (!sameTypes(functionType(module, creator).results, ["i64"])) throw new ValidationError("frame creators must return exactly one i64");
  }
  return true;
}
