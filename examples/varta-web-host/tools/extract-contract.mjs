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
  layout[r[0]] = range ? { start: Number(range[1]), end: Number(range[2]) } : { bytes: Number(r[1].split(" ")[0]) };
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
    maxDefinedFunctions: Number(r[4]),
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

writeFileSync(
  outPath,
  JSON.stringify({ profiles, acceptance, operations, layout, codes, reservedNames, limits, entryLimits, grantKinds, timers, schedulerErrors, schedulerMisuse, catalogTypes, toolCatalogExample, view }, null, 2) + "\n",
);
