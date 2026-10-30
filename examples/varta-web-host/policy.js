// Package manifest and host ceiling: parsing and resource authorization
// (embedding.md "Inputs a host receives", "Manifest", "Pattern matching").
import { parseStrictJson, exactKeys, JsonNumber } from "./strict-json.js";

export class PolicyError extends Error {}

const MAX_GRANTS = 128;
const MAX_PATTERNS = 256;
const MAX_PATTERNS_PER_GRANT = 64;
const MAX_PATTERN_BYTES = 512;
const MAX_MATCH_WORK = 65536;
const SCHEMA_VERSION = 1;
const DEFAULT_ABI_VERSION = 1;
const STRICT_POLICY = "strict";
const TARGETS = ["wasm", "native"];
const MANIFEST_FIELDS = ["schemaVersion", "abiVersion", "policy", "entry", "target", "budgets", "fsRoots"];
// Manifest members of host ABI 34 (embedding.md "Manifest"): `budgets`, `fsRoots`
// and the grant members `required` and `purpose` (package only).
const TURN_BUDGETS_ABI = 34;
const GRANT_V34_FIELDS = ["required", "purpose"];
const REQUEST_SCOPE_FIELDS = ["methods"];
// The contract names maxima for these members without publishing their values;
// until it does, this host refuses them rather than guess (fail closed).
const UNPUBLISHED_MAXIMA = {
  budgets: "budget::MAX_*",
  fsRoots: "files::MAX_FILE_ROOTS / MAX_ROOT_NAME_BYTES / MAX_PATH_BYTES",
  purpose: "permission::MAX_PURPOSE_BYTES",
  methods: "net::MAX_GRANT_METHODS",
};

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
function unpublished(where, member) {
  return new PolicyError(`${where}: contract publishes no maximum for ${member} (${UNPUBLISHED_MAXIMA[member]}); refused`);
}

export function parseManifest(text, { where = "manifest", hostAbis, grantKinds, ceiling = false }) {
  const grants = new Map();
  if (text === null) return Object.assign(grants, { abiVersion: undefined });
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
  if ("target" in doc && !TARGETS.includes(doc.target)) throw new PolicyError(`${where}: target must be "wasm" or "native"`);
  const v34 = abiVersion >= TURN_BUDGETS_ABI;
  for (const member of ["budgets", "fsRoots"]) {
    if (!(member in doc)) continue;
    if (!v34) throw new PolicyError(`${where}: ${member} needs abiVersion ${TURN_BUDGETS_ABI}`);
    throw unpublished(where, member);
  }
  if (!Array.isArray(doc.capabilities)) throw new PolicyError(`${where}: capabilities must be an array`);
  if (doc.capabilities.length > MAX_GRANTS) throw new PolicyError(`${where}: too many grants`);
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
      if (!v34 && GRANT_V34_FIELDS.some((f) => f in grant)) throw new PolicyError(`${where}: ${grant.id}.required/purpose need abiVersion ${TURN_BUDGETS_ABI}`);
      if (ceiling && GRANT_V34_FIELDS.some((f) => f in grant)) throw new PolicyError(`${where}: a host ceiling may not carry required or purpose`);
      throw new PolicyError(error.message);
    }
    if ("required" in grant && typeof grant.required !== "boolean") throw new PolicyError(`${where}: ${grant.id}.required must be a boolean`);
    if ("purpose" in grant) throw unpublished(where, "purpose");
    if ("methods" in grant) throw unpublished(where, "methods");
    if (kind === "boolean") {
      if (typeof grant.enabled !== "boolean") throw new PolicyError(`${where}: ${grant.id}.enabled must be a boolean`);
      grants.set(grant.id, { kind, enabled: grant.enabled, required: grant.required !== false });
      continue;
    }
    if (!Array.isArray(grant.patterns) || grant.patterns.some((p) => typeof p !== "string")) {
      throw new PolicyError(`${where}: ${grant.id}.patterns must be strings`);
    }
    if (grant.patterns.length > MAX_PATTERNS_PER_GRANT) throw new PolicyError(`${where}: too many patterns for ${grant.id}`);
    patternCount += grant.patterns.length;
    if (patternCount > MAX_PATTERNS) throw new PolicyError(`${where}: too many patterns`);
    for (const pattern of grant.patterns) {
      const reason = unsafeReason(pattern, kind);
      if (reason) throw new PolicyError(`${where}: unsafe ${grant.id} pattern ${JSON.stringify(pattern)}: ${reason}`);
    }
    grants.set(grant.id, { kind, patterns: grant.patterns, required: grant.required !== false });
  }
  return Object.assign(grants, { abiVersion });
}

// embedding.md "Manifest": unsafe patterns, per grant kind.
function unsafeReason(pattern, kind) {
  if (kind === "endpoints") return canonicalEndpoint(pattern) ? null : "not a canonical IP:port";
  if (utf8Length(pattern) > MAX_PATTERN_BYTES) return "longer than 512 bytes";
  if (pattern.trim() === "") return "empty or only whitespace";
  if (/^[*?]+$/.test(pattern)) return "only wildcards";
  if (pattern.includes("\0")) return "contains NUL";
  if (kind === "paths" && (!pattern.startsWith("/") || pattern.split("/").includes(".."))) return "not absolute or contains ..";
  if (kind === "urls") return unsafeUrl(pattern);
  return null;
}

function unsafeUrl(pattern) {
  const PREFIX = "http://";
  if (!pattern.startsWith(PREFIX)) return "does not start with http://";
  if (/[\u0000- \u007f]/.test(pattern)) return "control or whitespace character";
  const rest = pattern.slice(PREFIX.length);
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
export function matchPattern(pattern, input, { paths = false } = {}) {
  if (utf8Length(pattern) * utf8Length(input) > MAX_MATCH_WORK) return false;
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
  return grant.patterns.some((pattern) => matchPattern(pattern, resource, { paths: grant.kind === "paths" }));
}

/**
 * Package request intersected with the optional host ceiling. A supplied
 * ceiling must also allow the concrete resource; an empty one denies.
 */
export function authorize(packageGrants, ceilingGrants, id, resource = "") {
  if (!grantAllows(packageGrants, id, resource)) return false;
  return ceilingGrants === null || grantAllows(ceilingGrants, id, resource);
}
