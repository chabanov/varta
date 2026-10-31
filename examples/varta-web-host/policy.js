// Package manifest and host ceiling: parsing and resource authorization
// (embedding.md "Inputs a host receives", "Manifest", "Pattern matching").
import { parseStrictJson, exactKeys, JsonNumber } from "./strict-json.js";

export class PolicyError extends Error {}

const SCHEMA_VERSION = 1;
const DEFAULT_ABI_VERSION = 1;
const STRICT_POLICY = "strict";
const MANIFEST_FIELDS = ["schemaVersion", "abiVersion", "policy", "entry", "target", "budgets", "fsRoots"];
const V34_MEMBERS = ["budgets", "fsRoots"];
const GRANT_V34_FIELDS = ["required", "purpose"];
const REQUEST_SCOPE_FIELDS = ["methods"];
// embedding.md "Root names": ASCII, a lowercase first letter, then a–z, 0–9, `-`, `_`.
const ROOT_NAME = /^[a-z][a-z0-9_-]*$/;
// RFC 9110 `token`.
const METHOD_TOKEN = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
// Unicode `White_Space` and general category `Cc` (embedding.md "Purpose rule").
const WHITE_SPACE_ONLY = /^[\u0009-\u000d\u0020\u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]*$/u;
const CONTROL = /\p{Cc}/u;
const HTTP = "http://";
const HTTPS = "https://";
const ANY_HTTPS_LINK = "https://*";

/** Validates the contract's root-name examples against this host's grammar once. */
export function checkRootNameGrammar(limits, examples) {
  for (const example of examples) {
    if (validRootName(example.name, limits) !== example.valid) throw new Error(`root name grammar disagrees with the contract on ${JSON.stringify(example.name)}`);
  }
}

function validRootName(name, limits) {
  const bytes = utf8Length(name);
  return bytes >= limits.rootNameBytes.min && bytes <= limits.rootNameBytes.max && ROOT_NAME.test(name);
}

const utf8Length = (text) => new TextEncoder().encode(text).length;

function integer(value, where) {
  if (!(value instanceof JsonNumber) || !/^-?(0|[1-9][0-9]*)$/.test(value.raw)) {
    throw new PolicyError(`${where} must be an integer`);
  }
  return Number(value.raw);
}

/**
 * Parses a package manifest or a host ceiling (same schema). `null` means no
 * manifest: no grants. The result carries `abiVersion` (default 1); comparing it
 * with the module is the caller's job, and only for package manifests.
 * @param {string | null} text
 * Grant kinds come from the generated grant-kind table of embedding.md "Manifest".
 * @param {{where?: string, hostAbis: Set<number>, grantKinds: Record<string, string>}} options
 */
export function parseManifest(text, { where = "manifest", hostAbis, grantKinds, ceiling = false, budgetTable, limits }) {
  const grants = new Map();
  if (text === null) return Object.assign(grants, { abiVersion: undefined, limits });
  let doc;
  try {
    doc = parseStrictJson(text);
    exactKeys(doc, ["capabilities"], MANIFEST_FIELDS, where);
  } catch (error) {
    throw new PolicyError(`${where}: ${error.message}`);
  }
  if ("schemaVersion" in doc && integer(doc.schemaVersion, "schemaVersion") !== SCHEMA_VERSION) throw new PolicyError(`${where}: schemaVersion must be ${SCHEMA_VERSION}`);
  const abiVersion = "abiVersion" in doc ? integer(doc.abiVersion, "abiVersion") : DEFAULT_ABI_VERSION;
  if (!hostAbis.has(abiVersion)) throw new PolicyError(`${where}: unsupported abiVersion ${abiVersion}`);
  if ("policy" in doc && doc.policy !== STRICT_POLICY) throw new PolicyError(`${where}: policy must be "${STRICT_POLICY}"`);
  if ("entry" in doc && typeof doc.entry !== "string") throw new PolicyError(`${where}: entry must be a string`);
  if ("target" in doc && !limits.manifestTargets.includes(doc.target)) throw new PolicyError(`${where}: target must be one of ${limits.manifestTargets.join(", ")}`);
  const v34 = abiVersion >= limits.turnBudgetsAbi;
  for (const member of V34_MEMBERS) {
    if (member in doc && !v34) throw new PolicyError(`${where}: ${member} needs abiVersion ${limits.turnBudgetsAbi}`);
  }
  const budgets = "budgets" in doc ? parseBudgets(doc.budgets, budgetTable, where) : {};
  const fsRoots = "fsRoots" in doc ? parseRoots(doc.fsRoots, ceiling, limits, where) : null;
  // A v34 package names its roots; its fs.* patterns must start with one of them.
  const packageRoots = !ceiling && v34 ? new Set(fsRoots ?? []) : null;
  if (!Array.isArray(doc.capabilities)) throw new PolicyError(`${where}: capabilities must be an array`);
  if (doc.capabilities.length > limits.maxPolicyGrants) throw new PolicyError(`${where}: too many grants`);
  let patternCount = 0;
  for (const grant of doc.capabilities) {
    if (grant === null || typeof grant !== "object" || typeof grant.id !== "string") throw new PolicyError(`${where}: grant needs an id`);
    const kind = grantKinds[grant.id];
    if (!kind) throw new PolicyError(`${where}: unknown capability id ${grant.id}`);
    if (grants.has(grant.id)) throw new PolicyError(`${where}: duplicate capability id ${grant.id}`);
    const optional = [...(v34 && !ceiling ? GRANT_V34_FIELDS : []), ...(kind === "requests" ? REQUEST_SCOPE_FIELDS : [])];
    try {
      exactKeys(grant, kind === "boolean" ? ["id", "enabled"] : ["id", "patterns"], optional, `${where} grant ${grant.id}`);
    } catch (error) {
      if (!v34 && GRANT_V34_FIELDS.some((f) => f in grant)) throw new PolicyError(`${where}: ${grant.id}.required/purpose need abiVersion ${limits.turnBudgetsAbi}`);
      if (ceiling && GRANT_V34_FIELDS.some((f) => f in grant)) throw new PolicyError(`${where}: a host ceiling may not carry required or purpose`);
      throw new PolicyError(error.message);
    }
    if ("required" in grant && typeof grant.required !== "boolean") throw new PolicyError(`${where}: ${grant.id}.required must be a boolean`);
    if ("purpose" in grant) checkPurpose(grant.purpose, limits, `${where}: ${grant.id}.purpose`);
    const methods = "methods" in grant ? parseMethods(grant.methods, limits, `${where}: ${grant.id}.methods`) : null;
    if (kind === "boolean") {
      if (typeof grant.enabled !== "boolean") throw new PolicyError(`${where}: ${grant.id}.enabled must be a boolean`);
      grants.set(grant.id, { kind, enabled: grant.enabled, required: grant.required !== false });
      continue;
    }
    if (!Array.isArray(grant.patterns) || grant.patterns.some((p) => typeof p !== "string")) {
      throw new PolicyError(`${where}: ${grant.id}.patterns must be strings`);
    }
    if (grant.patterns.length > limits.maxGrantPatterns) throw new PolicyError(`${where}: too many patterns for ${grant.id}`);
    patternCount += grant.patterns.length;
    if (patternCount > limits.maxPolicyPatterns) throw new PolicyError(`${where}: too many patterns`);
    for (const pattern of grant.patterns) {
      const reason = unsafeReason(pattern, kind, limits) ?? (kind === "paths" && packageRoots ? unsafeRootedPath(pattern, packageRoots) : null);
      if (reason) throw new PolicyError(`${where}: unsafe ${grant.id} pattern ${JSON.stringify(pattern)}: ${reason}`);
    }
    grants.set(grant.id, { kind, patterns: grant.patterns, methods, required: grant.required !== false });
  }
  return Object.assign(grants, { abiVersion, budgets, fsRoots, limits });
}

/** `budgets`: known members only, each an integer in its published range. */
function parseBudgets(value, table, where) {
  if (value === null || typeof value !== "object" || Array.isArray(value) || value instanceof JsonNumber) throw new PolicyError(`${where}: budgets must be an object`);
  try {
    exactKeys(value, [], Object.keys(table), `${where} budgets`);
  } catch (error) {
    throw new PolicyError(error.message);
  }
  const budgets = {};
  for (const [member, raw] of Object.entries(value)) {
    const n = integer(raw, `${where}: budgets.${member}`);
    const { min, max } = table[member];
    if (n < min || n > max) throw new PolicyError(`${where}: budgets.${member} must be ${min} to ${max}`);
    budgets[member] = n;
  }
  return budgets;
}

/** `fsRoots`: an array of root names in a package, an object of host directories in a ceiling. */
function parseRoots(value, ceiling, limits, where) {
  if (ceiling) {
    if (value === null || typeof value !== "object" || Array.isArray(value) || value instanceof JsonNumber) throw new PolicyError(`${where}: fsRoots must map root names to directories`);
    const entries = Object.entries(value);
    if (entries.length > limits.maxFileRoots) throw new PolicyError(`${where}: too many fsRoots`);
    for (const [name, directory] of entries) {
      if (!validRootName(name, limits)) throw new PolicyError(`${where}: invalid root name ${JSON.stringify(name)}`);
      if (typeof directory !== "string" || !directory.startsWith("/") || directory.split("/").includes("..") || CONTROL.test(directory) || utf8Length(directory) > limits.maxPathBytes) {
        throw new PolicyError(`${where}: root ${name} must map to an absolute directory without .. or control characters, at most ${limits.maxPathBytes} bytes`);
      }
    }
    return Object.fromEntries(entries);
  }
  if (!Array.isArray(value)) throw new PolicyError(`${where}: fsRoots must be an array of root names`);
  if (value.length > limits.maxFileRoots) throw new PolicyError(`${where}: too many fsRoots`);
  const names = new Set();
  for (const name of value) {
    if (typeof name !== "string" || !validRootName(name, limits)) throw new PolicyError(`${where}: invalid root name ${JSON.stringify(name)}`);
    if (names.has(name)) throw new PolicyError(`${where}: duplicate root name ${name}`);
    names.add(name);
  }
  return [...names];
}

/** embedding.md "Purpose rule". */
function checkPurpose(purpose, limits, where) {
  if (typeof purpose !== "string" || WHITE_SPACE_ONLY.test(purpose) || CONTROL.test(purpose) || utf8Length(purpose) > limits.maxPurposeBytes) {
    throw new PolicyError(`${where} must be nonblank text without control characters, at most ${limits.maxPurposeBytes} bytes`);
  }
}

/** Request methods: RFC 9110 tokens, never CONNECT/TRACE/TRACK, the normalized six in uppercase, no duplicates. */
function parseMethods(methods, limits, where) {
  if (!Array.isArray(methods) || methods.some((m) => typeof m !== "string")) throw new PolicyError(`${where} must be strings`);
  if (methods.length > limits.maxGrantMethods) throw new PolicyError(`${where}: more than ${limits.maxGrantMethods} methods`);
  const seen = new Set();
  for (const method of methods) {
    if (!METHOD_TOKEN.test(method)) throw new PolicyError(`${where}: ${JSON.stringify(method)} is not a token`);
    const upper = method.toUpperCase();
    if (limits.forbiddenMethods.includes(upper)) throw new PolicyError(`${where}: ${method} is never grantable`);
    if (limits.normalizedMethods.includes(upper) && method !== upper) throw new PolicyError(`${where}: ${method} must be written ${upper}`);
    if (seen.has(method)) throw new PolicyError(`${where}: duplicate method ${method}`);
    seen.add(method);
  }
  return methods;
}

/** v34 packages: an fs.* pattern starts with a literal declared root and has only ordinary components. */
function unsafeRootedPath(pattern, roots) {
  const components = pattern.slice(1).split("/");
  if (!roots.has(components[0])) return "first component is not a root named by fsRoots";
  if (pattern.includes("\\") || CONTROL.test(pattern)) return "contains \\ or a control character";
  if (components.some((c) => c === "" || c === ".")) return "empty or . component";
  return null;
}

// embedding.md "Manifest": unsafe patterns, per grant kind.
function unsafeReason(pattern, kind, limits) {
  if (kind === "endpoints") return canonicalEndpoint(pattern) ? null : "not a canonical IP:port";
  if (utf8Length(pattern) > limits.maxPatternBytes) return `longer than ${limits.maxPatternBytes} bytes`;
  if (pattern.trim() === "") return "empty or only whitespace";
  if (/^[*?]+$/.test(pattern)) return "only wildcards";
  if (pattern.includes("\0")) return "contains NUL";
  if (kind === "paths" && (!pattern.startsWith("/") || pattern.split("/").includes(".."))) return "not absolute or contains ..";
  if (kind === "urls") return unsafeUrl(pattern, [HTTP]);
  if (kind === "requests") return unsafeUrl(pattern, [HTTP, HTTPS]);
  if (kind === "links") return pattern === ANY_HTTPS_LINK ? null : unsafeUrl(pattern, [HTTP, HTTPS]);
  return null;
}

function unsafeUrl(pattern, schemes) {
  const scheme = schemes.find((prefix) => pattern.startsWith(prefix));
  if (!scheme) return `does not start with ${schemes.join(" or ")}`;
  if (/[\u0000- \u007f]/.test(pattern)) return "control or whitespace character";
  const rest = pattern.slice(scheme.length);
  const slash = rest.indexOf("/");
  const authority = slash < 0 ? rest : rest.slice(0, slash);
  const path = slash < 0 ? "" : rest.slice(slash);
  if (authority === "" || /[*?@#]/.test(authority)) return "bad authority";
  if (/[#%\\]/.test(path)) return "path contains #, % or \\";
  if (path.split(/[/?]/).some((segment) => segment === "." || segment === "..")) return "dot path segment";
  let url;
  try {
    url = new URL(pattern.replaceAll("*", "x").replaceAll("?", "/"));
  } catch {
    return "not parseable as a URL";
  }
  if (!url.hostname || url.username || url.password || url.pathname.includes("%")) return "no host, credentials or % in path";
  return null;
}

function canonicalEndpoint(text) {
  const v4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3}):(\d{1,5})$/.exec(text);
  if (v4) {
    const parts = v4.slice(1, 5);
    const port = v4[5];
    const canonical = (n) => String(Number(n)) === n;
    return parts.every((p) => canonical(p) && Number(p) <= 255) && canonical(port) && Number(port) <= 65535;
  }
  const v6 = /^\[([0-9a-f:]+)\]:(\d{1,5})$/.exec(text);
  if (!v6) return false;
  try {
    return new URL(`http://[${v6[1]}]/`).hostname === `[${v6[1]}]` && String(Number(v6[2])) === v6[2] && Number(v6[2]) <= 65535;
  } catch {
    return false;
  }
}

/** Glob match per Unicode scalar value; fails closed on excessive work. */
export function matchPattern(pattern, input, { paths = false, maxWork }) {
  if (utf8Length(pattern) * utf8Length(input) > maxWork) return false;
  const p = [...pattern];
  const s = [...input];
  const memo = new Map();
  const go = (i, j) => {
    const key = i * (s.length + 1) + j;
    if (memo.has(key)) return memo.get(key);
    let result;
    if (i === p.length) result = j === s.length;
    else if (p[i] === "*") {
      const deep = paths && p[i + 1] === "*";
      const next = deep ? i + 2 : i + 1;
      result = go(next, j) || (j < s.length && (deep || !paths || s[j] !== "/") && go(i, j + 1));
    } else if (p[i] === "?") result = j < s.length && (!paths || s[j] !== "/") && go(i + 1, j + 1);
    else result = j < s.length && p[i] === s[j] && go(i + 1, j + 1);
    memo.set(key, result);
    return result;
  };
  return go(0, 0);
}

function grantAllows(grants, id, resource) {
  const grant = grants.get(id);
  if (!grant) return false;
  if (grant.kind === "boolean") return grant.enabled;
  return grant.patterns.some((pattern) => matchPattern(pattern, resource, { paths: grant.kind === "paths", maxWork: grants.limits.maxPatternMatchWork }));
}

/**
 * A URL against a URL pattern (embedding.md "Pattern matching"): scheme, normalized
 * host and effective port exactly; `*` after the authority matches any run, and `?`
 * keeps its query-separator meaning (a literal character).
 */
function urlMatches(pattern, url, maxWork) {
  const scheme = /^([a-z]+):\/\/([^/]*)(.*)$/.exec(pattern);
  if (!scheme) return false;
  let authority;
  try {
    authority = new URL(`${scheme[1]}://${scheme[2]}/`);
  } catch {
    return false;
  }
  if (authority.protocol !== url.protocol || authority.hostname !== url.hostname || authority.port !== url.port) return false;
  const rest = url.pathname + url.search;
  const want = scheme[3] === "" ? "/" : scheme[3];
  if (utf8Length(want) * utf8Length(rest) > maxWork) return false;
  const p = [...want];
  const s = [...rest];
  const memo = new Map();
  const go = (i, j) => {
    const key = i * (s.length + 1) + j;
    if (memo.has(key)) return memo.get(key);
    let result;
    if (i === p.length) result = j === s.length;
    else if (p[i] === "*") result = go(i + 1, j) || (j < s.length && go(i, j + 1));
    else result = j < s.length && p[i] === s[j] && go(i + 1, j + 1);
    memo.set(key, result);
    return result;
  };
  return go(0, 0);
}

function requestAllowed(grants, url, method) {
  const grant = grants.get("net.request");
  if (!grant || grant.required === false) return false;
  if (grant.methods !== null && grant.methods !== undefined && !grant.methods.includes(method)) return false;
  return grant.patterns.some((pattern) => urlMatches(pattern, url, grants.limits.maxPatternMatchWork));
}

/** `net.request`: the URL and method satisfy the package request and, when supplied, the ceiling. */
export function authorizeRequest(packageGrants, ceilingGrants, url, method) {
  if (!requestAllowed(packageGrants, url, method)) return false;
  return ceilingGrants === null || requestAllowed(ceilingGrants, url, method);
}

/**
 * Package request intersected with the optional host ceiling. A supplied
 * ceiling must also allow the concrete resource; an empty one denies.
 * Permission broker v1 (architecture.md "Permission broker v1"): request ∩ ceiling,
 * then the stored decision, then the prompt seam. This host stores no decisions and
 * has no prompt seam, so an optional grant (`required: false`) is denied.
 */
export function authorize(packageGrants, ceilingGrants, id, resource = "") {
  if (packageGrants.get(id)?.required === false) return false;
  if (!grantAllows(packageGrants, id, resource)) return false;
  return ceilingGrants === null || grantAllows(ceilingGrants, id, resource);
}
