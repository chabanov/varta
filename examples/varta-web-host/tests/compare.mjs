// Differential test: the JS host against the native reference runtime.
// Builds every fixture with the native `varta`, records the native observations in
// tests/out/expected.json (also used by the browser runner), runs the same cases in
// the JS host under Node and compares.
//
//   VARTA_BIN=/path/to/varta node tests/compare.mjs
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createHost } from "../host.js";
import { invokeCases, runCases, refusalCases, schedulerCases, manifestCases, MANIFEST_FIXTURE, comparableInvoke, comparableRun, runJsCases } from "./cases.js";

const varta = process.env.VARTA_BIN;
if (!varta) throw new Error("set VARTA_BIN to the native varta binary");
const here = dirname(fileURLToPath(import.meta.url));
const fixtures = join(here, "..", "fixtures");
const contract = JSON.parse(readFileSync(join(here, "..", "contract.json"), "utf8"));
const host = createHost(contract);
const scratch = mkdtempSync(join(tmpdir(), "varta-host-compare-"));
const REFERENCE_PREFIXES = ["varta log: ", "varta env: "];
const COMPLETED = "varta run: completed";
const FAILED = "varta failed: ";
const MANIFEST_REFUSAL = "invalid capability manifest";

const built = new Map();
function build(name) {
  if (!built.has(name)) {
    execFileSync(varta, ["build"], { cwd: join(fixtures, name), stdio: "pipe" });
    const wasm = join(fixtures, name, "dist", "app.wasm");
    built.set(name, { wasm, bytes: new Uint8Array(readFileSync(wasm)), manifest: readFileSync(wasm.replace(/\.wasm$/, ".manifest.json"), "utf8") });
  }
  return built.get(name);
}

// `varta run` prints reference lines and ends with "varta run: completed …"; an
// authorized guest exit becomes the process exit status.
function nativeRun(wasm, c) {
  const out = spawnSync(varta, ["run", wasm], { env: { PATH: process.env.PATH, ...c.env }, encoding: "utf8" });
  const lines = out.stdout.split("\n");
  const logs = lines.filter((l) => REFERENCE_PREFIXES.some((p) => l.startsWith(p)));
  const completed = `${out.stdout}${out.stderr}`.includes(COMPLETED);
  if (completed && out.status !== 0) return comparableRun({ status: "exit", exitCode: out.status, logs }, c);
  const failure = `${out.stdout}\n${out.stderr}`.split("\n").find((l) => l.startsWith(FAILED));
  return comparableRun({ status: out.status === 0 ? "ok" : "failed", logs, error: failure?.slice(FAILED.length) }, c);
}

function nativeInvoke(wasm, c) {
  const args = ["invoke", "--json", wasm, c.tool, "--params", c.params];
  if (c.ceiling) {
    const policy = join(scratch, "ceiling.json");
    writeFileSync(policy, c.ceiling);
    args.push("--policy", policy);
  }
  const out = spawnSync(varta, args, { env: { PATH: process.env.PATH, ...(c.env ?? {}) }, encoding: "utf8" });
  return comparableInvoke(JSON.parse(out.stdout.split("\n")[0]));
}

const expected = [];
const tools = build("tools-i32");
for (const c of invokeCases) expected.push({ label: `T1 ${c.label}`, expected: nativeInvoke(tools.wasm, c) });
for (const c of runCases) expected.push({ label: c.label, expected: nativeRun(build(c.fixture).wasm, c) });
for (const c of refusalCases) {
  build(c.fixture);
  expected.push({ label: c.label, expected: c.expect ?? "refused" });
}
for (const c of schedulerCases) expected.push({ label: `contract check, scheduler misuse: ${c.label}`, expected: c.expect });
// Manifest cases: the native runtime reads the sidecar next to the module.
for (const c of manifestCases) {
  const manifestFixture = build(c.fixture ?? MANIFEST_FIXTURE);
  const dir = mkdtempSync(join(scratch, "manifest-"));
  const wasm = join(dir, "app.wasm");
  writeFileSync(wasm, manifestFixture.bytes);
  writeFileSync(join(dir, "app.manifest.json"), JSON.stringify(c.edit(JSON.parse(manifestFixture.manifest))));
  const out = spawnSync(varta, ["run", wasm], { env: { PATH: process.env.PATH }, encoding: "utf8" });
  const error = `${out.stdout}${out.stderr}`.split("\n").find((l) => l.startsWith(FAILED)) ?? null;
  const outcome = out.status === 0 ? "accepted" : error?.startsWith(`${FAILED}${MANIFEST_REFUSAL}`) ? "refused" : "run failed";
  expected.push({ label: c.label, expected: outcome, nativeError: error });
}
mkdirSync(join(here, "out"), { recursive: true });
writeFileSync(join(here, "out", "expected.json"), JSON.stringify(expected, null, 2) + "\n");

const observed = await runJsCases(host, async (name) => build(name), contract);
let failures = 0;
expected.forEach(({ label, expected: want }, i) => {
  const got = observed[i].observed;
  const ok = JSON.stringify(want) === JSON.stringify(got);
  if (!ok) failures += 1;
  const note = expected[i].nativeError ? `\n  native error: ${expected[i].nativeError}` : "";
  console.log(ok ? `ok   ${label}` : `FAIL ${label}\n  native: ${JSON.stringify(want)}\n  js:     ${JSON.stringify(got)}${note}`);
});

rmSync(scratch, { recursive: true, force: true });
console.log(`\n${expected.length - failures} passed, ${failures} failed`);
process.exitCode = failures ? 1 : 0;
