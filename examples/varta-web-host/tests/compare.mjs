// Differential test: the JS host against the native reference runtime.
// Builds every fixture with the native `varta`, records the native observations in
// tests/out/expected.json (also used by the browser runner), runs the same cases in
// the JS host under Node and compares.
//
//   VARTA_BIN=/path/to/varta node tests/compare.mjs
import { execFileSync, spawnSync, spawn } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createHost } from "../host.js";
import { invokeCases, runCases, refusalCases, schedulerCases, manifestCases, viewCases, MANIFEST_FIXTURE, comparableInvoke, comparableRun, runJsCases } from "./cases.js";

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

// View applications: the native headless tree after the scripted inputs, for the
// browser page tests/view.html (a JavaScript host renders only in a browser).
const viewExpected = [];
for (const c of viewCases) {
  build(c.fixture);
  const dir = join(fixtures, c.fixture);
  const scriptPath = join(scratch, `${c.fixture}.script.json`);
  writeFileSync(scriptPath, JSON.stringify(c.script));
  const out = spawnSync(varta, ["dev", "--view-script", scriptPath], { cwd: dir, encoding: "utf8" });
  const text = out.stdout.slice(0, out.stdout.lastIndexOf("}") + 1);
  viewExpected.push({ label: c.label, fixture: c.fixture, script: c.script, tree: JSON.parse(text).tree });
}
writeFileSync(join(here, "out", "view-expected.json"), JSON.stringify(viewExpected, null, 2) + "\n");

const observed = await runJsCases(host, async (name) => build(name), contract);
// Network requests: one loopback server, the grant rewritten to its port, the
// same module and manifest in the native runtime and in the JS host (Node only:
// the browser page has no server to reach).
const NET_FIXTURE = "net-v38";
const SPLIT_LETTER = Buffer.from("в", "utf8");
const server = createServer((req, res) => {
  if (req.url === "/json") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end('{"ok":true}');
  } else if (req.url === "/headers") {
    // Repeats and names out of order, as the wire carries them (raw flat list);
    // no Date field, which could differ between the native and the JS run.
    res.sendDate = false;
    res.writeHead(200, ["x-b", "1", "content-type", "text/plain", "set-cookie", "a=1", "x-a", "z", "x-b", "2", "set-cookie", "b=2"]);
    res.end("h");
  } else if (req.url === "/stream") {
    res.writeHead(200, { "content-type": "text/plain; charset=utf-8" });
    res.write(Buffer.concat([Buffer.from("при", "utf8"), SPLIT_LETTER.subarray(0, 1)]));
    setTimeout(() => res.end(Buffer.concat([SPLIT_LETTER.subarray(1), Buffer.from("іт", "utf8")])), 30);
  } else if (req.url === "/echo") {
    const parts = [];
    req.on("data", (chunk) => parts.push(chunk));
    req.on("end", () => {
      res.writeHead(200, { "content-type": "text/plain; charset=utf-8" });
      res.end(Buffer.concat(parts));
    });
  } else if (req.url === "/large") {
    res.writeHead(200, { "content-type": "text/plain" });
    res.end("x".repeat(600000));
  } else {
    res.writeHead(404, { "content-type": "text/plain" });
    res.end("nope");
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const base = `http://127.0.0.1:${server.address().port}`;
const netFixture = build(NET_FIXTURE);
const netManifest = JSON.parse(netFixture.manifest);
netManifest.capabilities = netManifest.capabilities.map((g) => (g.id === "net.request" ? { ...g, patterns: [`${base}/**`] } : g));
const netDir = mkdtempSync(join(scratch, "net-"));
writeFileSync(join(netDir, "app.wasm"), netFixture.bytes);
writeFileSync(join(netDir, "app.manifest.json"), JSON.stringify(netManifest));
// The native run must not block this process: the server answers from this event loop.
const nativeNet = await new Promise((resolve) => {
  const child = spawn(varta, ["run", join(netDir, "app.wasm")], { env: { PATH: process.env.PATH, BASE_URL: base } });
  let stdout = "";
  child.stdout.on("data", (d) => (stdout += d));
  child.on("close", (status) => resolve({ status, logs: stdout.split("\n").filter((l) => REFERENCE_PREFIXES.some((p) => l.startsWith(p))) }));
});
const jsNet = await host.run(netFixture.bytes, { manifest: JSON.stringify(netManifest), env: new Map([["BASE_URL", base]]) });
server.close();
expected.push({ label: "v38 net.request: json, repeated headers, split UTF-8 stream, 404, POST, runtime header, too large, denied", expected: { status: nativeNet.status === 0 ? "ok" : "failed", logs: nativeNet.logs } });
observed.push({ label: "v38 net.request", observed: { status: jsNet.status, logs: jsNet.logs } });

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
