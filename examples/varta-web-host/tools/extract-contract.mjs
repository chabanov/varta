// Extracts the generated tables of docs/embedding.md and the resource limits of
// docs/architecture.md into contract.json. The host reads only this data, so it is
// built from the published contract and never from engine sources.
//
// Usage: node extract-contract.mjs <embedding.md> <architecture.md> <out.json>
import { readFileSync, writeFileSync } from "node:fs";

const [embeddingPath, architecturePath, outPath] = process.argv.slice(2);
if (!embeddingPath || !architecturePath || !outPath) {
  throw new Error("usage: extract-contract.mjs <embedding.md> <architecture.md> <out.json>");
}
const embedding = readFileSync(embeddingPath, "utf8");
const architecture = readFileSync(architecturePath, "utf8");

function block(text, name) {
  const begin = text.indexOf(`<!-- generated:${name}:begin`);
  const end = text.indexOf(`<!-- generated:${name}:end`);
  if (begin < 0 || end < begin) throw new Error(`missing generated block ${name}`);
  return text.slice(begin, end);
}

function rows(text) {
  return text
    .split("\n")
    .filter((line) => line.startsWith("|") && !line.startsWith("| ---"))
    .slice(1)
    .map((line) => line.slice(1, -1).split(" | ").map((cell) => cell.trim()));
}

const code = (cell) => {
  const match = /^`([^`]+)`$/.exec(cell);
  if (!match) throw new Error(`expected a code cell, got ${cell}`);
  return match[1];
};
const types = (cell) => (cell === "" ? [] : cell.split(",").map((t) => t.trim()));

function signature(cell) {
  const match = /^\(([^)]*)\) → \(([^)]*)\)$/.exec(cell);
  if (!match) throw new Error(`bad signature ${cell}`);
  return { params: types(match[1]), results: types(match[2]) };
}

const profiles = rows(block(embedding, "embedding-profiles")).map((r) => {
  const exception = r[6] === "—" ? null : r[6];
  const version = exception && /^`([^`]+)` = (\d+)$/.exec(exception);
  return {
    name: code(r[0]),
    hostAbi: Number(r[1]),
    valueAbi: Number(r[2]),
    numbers: r[3],
    strings: r[4],
    toolHttp: r[5] === "yes",
    exceptionChannel: exception !== null,
    exceptionVersion: version ? { global: version[1], value: Number(version[2]) } : null,
    frames: r[7] === "—" ? null : { catalog: Number(/catalog (\d+)/.exec(r[7])[1]) },
    scheduler: r[8] === "yes",
    admittedImports: Number(r[9]),
  };
});
const profileIndex = new Map(profiles.map((p, i) => [p.name, i]));

function admitted(cell) {
  const names = [];
  // A registered operation that no profile imports (it is linked into the guest).
  if (cell === "—") return names;
  for (const run of cell.split(", ")) {
    const parts = run.split(" – ").map(code);
    const from = profileIndex.get(parts[0]);
    const to = profileIndex.get(parts[parts.length - 1]);
    if (from === undefined || to === undefined || to < from) throw new Error(`bad run ${run}`);
    for (let i = from; i <= to; i += 1) names.push(profiles[i].name);
  }
  return names;
}

const operations = rows(block(embedding, "embedding-host-operations")).map((r) => {
  const capability = r[3].startsWith("`") ? code(r[3]) : null;
  return {
    name: code(r[0]),
    ...signature(r[1]),
    minHostAbi: Number(r[2]),
    capability,
    scheduler: r[3] === "none (scheduler)",
    resultGlobals: r[4] === "yes",
    stringResult: r[5] === "yes",
    admittedUnder: admitted(r[6]),
  };
});

const layout = {};
for (const r of rows(block(embedding, "embedding-layout"))) {
  const range = /^\[(\d+), (\d+)\)$/.exec(r[1]);
  // v38: the static extension `[base, base + n·page), n ≤ max` and the intrinsic
  // region after it, with its stack and arena sizes.
  const extension = /^\[(\d+), \d+ \+ n·(\d+)\), n ≤ (\d+)$/.exec(r[1]);
  const intrinsics = /, stack (\d+) bytes, arena (\d+) bytes, n \+ m ≤ (\d+)$/.exec(r[1]);
  if (range) layout[r[0]] = { start: Number(range[1]), end: Number(range[2]) };
  else if (extension) layout.staticExtension = { base: Number(extension[1]), pageBytes: Number(extension[2]), maxPages: Number(extension[3]) };
  else if (intrinsics) layout.intrinsicRegion = { stackBytes: Number(intrinsics[1]), arenaBytes: Number(intrinsics[2]), maxTotalPages: Number(intrinsics[3]) };
  else layout[r[0]] = { bytes: Number(r[1].split(" ")[0]) };
}

const codes = rows(block(embedding, "embedding-codes")).map((r) => ({
  channel: r[0].replaceAll("`", ""),
  code: Number(r[1]),
  name: code(r[2]),
}));

const reservedNames = rows(block(embedding, "embedding-names")).map((r) => ({
  name: code(r[0]),
  kind: r[1],
  constant: code(r[3]),
}));

const UNITS = { "KiB": 1024, "MiB": 1024 * 1024, "bytes": 1, "s": 1 };
function quantities(cell) {
  return cell.split(" / ").map((part, index, all) => {
    const [value, unit] = part.trim().split(" ");
    const last = all[all.length - 1].trim().split(" ")[1];
    return Number(value) * (UNITS[unit ?? last] ?? 1);
  });
}
const limits = {};
for (const r of rows(block(architecture, "limits"))) {
  limits[r[0]] = { values: quantities(r[1]), source: r[2] };
}

const entryLimits = {};
for (const r of rows(block(embedding, "embedding-entry-limits"))) {
  entryLimits[r[0]] = { values: r[1].split(" / ").map((v) => Number(v.split(" ")[0])), source: r[2] };
}

const toolCatalogExample = JSON.parse(
  /```json\n([\s\S]*?)```/.exec(block(embedding, "embedding-tool-catalog-example"))[1],
);

// Grant kind per capability id (generated from the second contract round on).
const GRANT_KIND_NAMES = {
  Boolean: "boolean",
  "Name/path patterns": "names",
  "URL patterns": "urls",
  "Listen endpoints": "endpoints",
  "Request scopes (v37)": "requests",
  "External-link patterns (v38)": "links",
};
const grantKinds = {};
if (embedding.includes("<!-- generated:embedding-grant-kinds:begin")) {
  for (const r of rows(block(embedding, "embedding-grant-kinds"))) {
    const kind = GRANT_KIND_NAMES[r[0]];
    if (!kind) throw new Error(`unknown grant kind ${r[0]}`);
    for (const id of [...r[2].matchAll(/`([^`]+)`/g)].map((m) => m[1])) {
      grantKinds[id] = kind === "names" && id.startsWith("fs.") ? "paths" : kind;
    }
  }
}

// Timer constants (event loop of scheduler profiles with timer imports).
// Reference diagnostics of fatal scheduler errors (placeholders in angle brackets).
const schedulerErrors = {};
if (embedding.includes("<!-- generated:embedding-scheduler-errors:begin")) {
  for (const r of rows(block(embedding, "embedding-scheduler-errors"))) schedulerErrors[code(r[0])] = code(r[1]);
}

// Which misuse of a scheduler import raises which error, in check order (round 8).
const schedulerMisuse = embedding.includes("<!-- generated:embedding-scheduler-misuse:begin")
  ? rows(block(embedding, "embedding-scheduler-misuse")).map((r) => ({
      imports: [...r[0].matchAll(/`([^`]+)`/g)].map((m) => m[1]),
      step: r[0].replaceAll("`", ""),
      misuse: r[1],
      error: code(r[2]),
    }))
  : [];

// Members of each catalog type kind besides `kind` (round 8).
const catalogTypes = {};
if (embedding.includes("<!-- generated:embedding-catalog-types:begin")) {
  for (const r of rows(block(embedding, "embedding-catalog-types"))) {
    const member = /^`([a-zA-Z]+)`:/.exec(r[1]);
    catalogTypes[code(r[0])] = r[1] === "none"
      ? { members: [] }
      : { members: [member[1]], nullable: /or `null`/.test(r[1]), mayBeAbsent: /may be absent/.test(r[1]), sortedByName: /sorted by `name`/.test(r[1]) };
  }
}

const timers = {};
if (embedding.includes("<!-- generated:embedding-timers:begin")) {
  for (const r of rows(block(embedding, "embedding-timers"))) timers[r[2].replaceAll("`", "")] = r[1];
}

// Acceptance details per profile: accepted value ABIs, catalog versions, function limits.
const acceptance = {};
for (const r of rows(block(embedding, "embedding-profile-acceptance"))) {
  const catalogs = r[2] === "—" ? [] : r[2].split(", ").map((part) => {
    const match = /^(\d+)(?: \((no frames|frames)\))?$/.exec(part);
    if (!match) throw new Error(`bad catalog versions ${r[2]}`);
    return { version: Number(match[1]), frames: match[2] === "frames" };
  });
  acceptance[code(r[0])] = {
    values: r[1].split(", ").map(Number),
    catalogs,
    helperAllowance: Number(r[3]),
    // v38 counts defined and imported functions together ("N with imports").
    maxDefinedFunctions: Number(r[4].split(" ")[0]),
    functionLimitCountsImports: r[4].endsWith(" with imports"),
  };
}

// View protocol draft v0 (unstable): layout facts and code tables.
const viewLayout = {};
for (const r of rows(block(embedding, "view-layout"))) viewLayout[r[0]] = r[1];
const codeTable = (name) => Object.fromEntries(rows(block(embedding, name)).map((r) => [code(r[1]), Number(r[0])]));
const OPERAND = /^(\w+) \((u32 id|u16 code \+ 2 pad|string|u32|f64)\)$/;
const view = {
  layout: viewLayout,
  opcodes: rows(block(embedding, "view-opcodes")).map((r) => ({
    opcode: Number(r[0]),
    name: code(r[1]),
    operands: r[2].split(", ").map((operand) => {
      const match = OPERAND.exec(operand);
      if (!match) throw new Error(`bad operand ${operand}`);
      const kind = { "u32 id": "id", "u16 code + 2 pad": "code", string: "string", u32: "u32", f64: "f64" }[match[2]];
      return { name: match[1], kind };
    }),
  })),
  tags: codeTable("view-tags"),
  attributes: codeTable("view-attributes"),
  cssProperties: codeTable("view-css-properties"),
  events: codeTable("view-events"),
  commands: codeTable("view-commands"),
};

// Optional detail columns (present from the second draft v0 round on).
const detailRows = (name) => (embedding.includes(`<!-- generated:${name}:begin`) ? rows(block(embedding, name)) : []);
const unescapeCell = (cell) => cell.replaceAll("\\|", "|");
const codesIn = (cell) => [...cell.matchAll(/`([^`]+)`/g)].map((m) => m[1]);
view.attributeRules = Object.fromEntries(detailRows("view-attributes").filter((r) => r.length >= 4).map((r) => [
  code(r[1]),
  { elements: r[2] === "all" ? "all" : codesIn(r[2]), value: unescapeCell(r[3]) },
]));
view.cssGrammar = Object.fromEntries(detailRows("view-css-properties").filter((r) => r.length >= 3).map((r) => [code(r[1]), unescapeCell(r[2]).replace(/^`|`$/g, "")]));
view.cssNotes = Object.fromEntries(detailRows("view-css-properties").filter((r) => r.length >= 4 && r[3] !== "—").map((r) => [code(r[1]), r[3]]));
view.keys = Object.fromEntries(detailRows("view-keys").map((r) => [code(r[1]), { code: Number(r[0]), domKey: /^`(.*)`$/.exec(r[2])[1] }]));
view.capabilities = Object.fromEntries(detailRows("view-capabilities").map((r) => [code(r[0]), { grant: r[1], authorizes: r[2] }]));
view.defaults = Object.fromEntries(detailRows("view-defaults").map((r) => [r[0], codesIn(r[1])]));
view.commandResults = Object.fromEntries(detailRows("view-command-results").filter((r) => /^\d+$/.test(r[0])).map((r) => [code(r[1]), { status: Number(r[0]), operations: codesIn(r[2]) }]));
view.cssValueTypes = Object.fromEntries(detailRows("view-css-values").map((r) => [code(r[0]), unescapeCell(r[1])]));
view.stylesheetTokens = Object.fromEntries(detailRows("view-stylesheet-tokens").map((r) => [r[0], codesIn(r[1])]));
view.eventRules = Object.fromEntries(detailRows("view-events").filter((r) => r.length >= 4).map((r) => [code(r[1]), { coalescible: r[2] === "yes", incrementsSeq: r[3] === "yes" }]));
view.commandRules = Object.fromEntries(detailRows("view-commands").filter((r) => r.length >= 4).map((r) => [
  code(r[1]),
  { elements: r[2] === "all" ? "all" : r[2] === "focusable elements" ? "focusable" : codesIn(r[2]), argument: r[3] },
]));

// Manifest budgets and limits (v38 contract round of `bc4e104`).
const manifestBudgets = {};
for (const r of detailRows("embedding-manifest-budgets")) {
  const range = /^(\d+) to (\d+)$/.exec(r[1]);
  if (!range) throw new Error(`bad budget range ${r[1]}`);
  manifestBudgets[code(r[0])] = { min: Number(range[1]), max: Number(range[2]), turnDefault: Number(r[2]), executionValue: Number(r[3]) };
}
const MANIFEST_LIMIT_KEYS = {
  "`abiVersion` that admits `budgets`, grant `required` and `purpose`, and `fsRoots`": "turnBudgetsAbi",
  "Grant `purpose` (UTF-8 bytes)": "maxPurposeBytes",
  "`fsRoots` entries (package names or host mappings)": "maxFileRoots",
  "Root name (bytes)": "rootNameBytes",
  "Host root directory / guest virtual path (bytes)": "maxPathBytes",
  "Methods in one `net.request` grant": "maxGrantMethods",
  "Methods never grantable (in any case)": "forbiddenMethods",
  "Methods a grant must spell in uppercase": "normalizedMethods",
  "Guest URL of `net_get*`, `net_fetch*` and `net_request_*` (bytes; a longer URL is denied)": "maxUrlBytes",
  "Grants in one manifest or host policy": "maxPolicyGrants",
  "Patterns, listen endpoints and request URLs across all grants": "maxPolicyPatterns",
  "Patterns, listen endpoints or request URLs in one grant": "maxGrantPatterns",
  "Name, path or URL pattern (bytes)": "maxPatternBytes",
  "Pattern bytes × input bytes of one comparison (more fails closed)": "maxPatternMatchWork",
  "`target` values": "manifestTargets",
  "Manifest, host ceiling or decision file the reference CLI and launcher read (bytes)": "maxManifestBytes",
  "Response head of `net_get*` and `net_fetch*` (bytes; a larger head is an I/O failure)": "maxFetchHeaderBytes",
  "Environment variable name of `env_*` (bytes; nonempty, a longer name is denied)": "maxEnvNameBytes",
  "Absolute host path of `fs_*` before v38 (bytes; a longer path is denied)": "maxHostPathBytes",
};
const manifestLimits = {};
for (const r of detailRows("embedding-manifest-limits")) {
  const key = MANIFEST_LIMIT_KEYS[r[0]];
  if (!key) throw new Error(`unknown manifest limit ${r[0]}`);
  const range = /^(\d+) to (\d+)$/.exec(r[1]);
  const atLeast = /^(\d+) or later$/.exec(r[1]);
  if (range) manifestLimits[key] = { min: Number(range[1]), max: Number(range[2]) };
  else if (atLeast) manifestLimits[key] = Number(atLeast[1]);
  else if (r[1].startsWith("`")) manifestLimits[key] = codesIn(r[1]);
  else manifestLimits[key] = Number(r[1]);
}
// Host limit tables (module, scheduler, events, network, permissions): every row as
// { limit, profiles, value, sources }. A row whose label ends in a profile or a
// profile run applies to those profiles only; the host picks the row by profile.
function profileRun(text) {
  const names = codesIn(text);
  if (names.length === 0 || names.some((n) => !profileIndex.has(n))) return null;
  const from = profileIndex.get(names[0]);
  const to = profileIndex.get(names[names.length - 1]);
  return profiles.slice(from, to + 1).map((p) => p.name);
}
const hostLimits = [];
for (const name of ["embedding-module-limits", "embedding-scheduler-limits", "embedding-event-limits", "embedding-net-limits", "embedding-permission-limits"]) {
  for (const r of detailRows(name)) {
    const comma = r[0].lastIndexOf(", `");
    const run = comma < 0 ? null : profileRun(r[0].slice(comma + 2));
    const limit = run ? r[0].slice(0, comma) : r[0];
    const value = /^\d+$/.test(r[1]) ? Number(r[1]) : r[1];
    hostLimits.push({ table: name, limit, profiles: run, value, sources: codesIn(r[2]) });
  }
}

// Normative examples of the root name grammar; the host checks its grammar against them.
const rootNameExamples = detailRows("embedding-root-names").map((r) => {
  const repeated = /^`(.)` repeated (\d+) times$/.exec(r[0]);
  const name = r[0] === "(empty)" ? "" : repeated ? repeated[1].repeat(Number(repeated[2])) : code(r[0]);
  return { name, valid: r[1] === "yes" };
});

writeFileSync(
  outPath,
  JSON.stringify({ profiles, acceptance, operations, layout, codes, reservedNames, limits, entryLimits, grantKinds, timers, schedulerErrors, schedulerMisuse, catalogTypes, toolCatalogExample, view, manifestBudgets, manifestLimits, rootNameExamples, hostLimits }, null, 2) + "\n",
);
