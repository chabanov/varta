// Real-Chrome acceptance of the v38 network contract for long-lived SDK
// streams and host-owned headers. One program
// (crates/varta-runtime/src/net_cases/v38/long_stream.ts) runs as a long-lived
// application instance natively (`varta dev --app`) and in local Chrome (its
// own `varta build --target browser` bundle, mounted) against the same
// loopback server (`net_cases/server.mjs` routes, with CORS for the page's
// other origin). Its event stream lasts several test-only per-turn deadlines
// (the inactivity window) with a chunk inside every window and carries more
// than 512 KiB; a stall longer than one window fails the read; an SDK
// User-Agent is ignored by both hosts. A second program
// (net_cases/v38/burst.ts) runs under the production budgets: back-to-back
// events far larger than one read may deliver (net::MAX_READ_DELIVERY_BYTES),
// so each host must split them across reads instead of exhausting a turn's
// fuel. Both hosts must print the same lines. Nothing leaves 127.0.0.1.
// Usage: node scripts/test-browser-streaming.mjs [varta-executable]
import { createServer } from "node:http";
import { mkdir, mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { join, resolve, sep } from "node:path";
import { tmpdir } from "node:os";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import assert from "node:assert/strict";
import { withBrowserPage } from "./browser-probe.mjs";
import { event, route } from "../crates/varta-runtime/src/net_cases/server.mjs";
import { registry } from "../crates/varta-common/browser/registry.mjs";

// The parameters of `net_tests.rs` (WINDOW_MS, EVENT_*, STALL_GAP_MS).
const WINDOW_MS = 500, COUNT = 144, SIZE = 4096, PAUSE_MS = 12, GAP_MS = 3 * WINDOW_MS, STREAM_WINDOWS = 3;
assert(PAUSE_MS < WINDOW_MS && COUNT * PAUSE_MS >= STREAM_WINDOWS * WINDOW_MS);
const streamed = Array.from({ length: COUNT }, (_, i) => event(i + 1, SIZE).length).reduce((a, b) => a + b, 0);
// More than the v37 whole-body bound (net::MAX_RESPONSE_BODY_BYTES).
assert(streamed > registry.network.maxBodyBytes);
const expected = [
  'varta log: agent: 200 {"guestAgent":false}',
  "varta log: compressed: 200 compressed text no coding",
  `varta log: events: 200 ${streamed} bytes, last id: ${COUNT}`,
  "varta log: stall: TypeError terminated after data",
];
// The burst of `net_tests.rs` (BURST_COUNT, BURST_BYTES): every event is
// larger than one read's cap.
const BURST_COUNT = 12, BURST_SIZE = 60000, CAP = registry.network.maxReadBytes;
assert(BURST_SIZE > CAP);
const burst = Array.from({ length: BURST_COUNT }, (_, i) => event(i + 1, BURST_SIZE).length).reduce((a, b) => a + b, 0);
const burstExpected = [
  `varta log: bytes: 200 ${burst} bytes, last id: ${BURST_COUNT}, reads within cap true`,
  `varta log: text: 200 ${burst} units, last id: ${BURST_COUNT}`,
];

const executable = resolve(process.argv[2] ?? "target/debug/varta");
const root = await mkdtemp(join(tmpdir(), "varta-browser-streaming-"));
const run = promisify(execFile);
// The network origin differs from the page origin: CORS, as for a real API.
const cors = { "access-control-allow-origin": "*", "access-control-expose-headers": "content-encoding, content-length" };
const agents = [], codings = [];
const network = createServer((req, res) => {
  if (req.method === "OPTIONS") { res.writeHead(204, { ...cors, "access-control-allow-headers": "accept", "access-control-allow-methods": "GET" }).end(); return; }
  if (req.url === "/agent") agents.push(req.headers["user-agent"] ?? null);
  if (req.url === "/compressed") codings.push(req.headers["accept-encoding"] ?? null);
  route(req, res, cors).catch(() => res.destroy());
});
// The bundle the page server serves: that of the program `bothHosts` runs.
let served = root;
const pages = createServer(async (req, res) => {
  try {
    const name = decodeURIComponent(new URL(req.url, "http://loopback").pathname);
    if (name === "/blank.html") { res.writeHead(200, { "content-type": "text/html" }).end("<!doctype html><title>probe</title>"); return; }
    const path = resolve(served, `.${name}`);
    if (!path.startsWith(served + sep)) throw new Error("outside the bundle");
    const bytes = await readFile(path);
    res.writeHead(200, { "cache-control": "no-store", "content-type": path.endsWith(".js") ? "text/javascript" : path.endsWith(".html") ? "text/html" : "application/octet-stream" }).end(bytes);
  } catch { res.writeHead(404).end(); }
});
const listen = (server) => new Promise((done) => server.listen(0, "127.0.0.1", () => done(`http://127.0.0.1:${server.address().port}`)));
// Build `source` as a project with `budgets` (none: production), run it
// natively (`varta dev --app`) and in Chrome; both hosts' log lines.
async function bothHosts(name, source, budgets) {
  const project = join(root, name);
  await mkdir(project);
  await writeFile(join(project, "main.ts"), source);
  await writeFile(join(project, "varta.config.json"), JSON.stringify({
    entry: "main.ts", target: "wasm", out: "dist/app", languageProfile: "checked-f64-utf16-v38",
    capabilities: { net: { request: { urls: [`${base}/**`], methods: ["GET"], responseHeaders: ["content-encoding", "content-length", "content-type"] } } },
    ...(budgets ? { budgets } : {}),
  }));
  const started = Date.now();
  const native = await run(executable, ["dev", "--app"], { cwd: project, maxBuffer: 1 << 20 });
  const nativeElapsed = Date.now() - started;
  const nativeLines = native.stdout.split("\n").filter((line) => line.startsWith("varta log:"));
  await run(executable, ["build", "--target", "browser"], { cwd: project, maxBuffer: 1 << 20 });
  served = join(project, "dist", "web");
  const browser = await withBrowserPage(`${pageBase}/blank.html`, async (call) => {
    const opened = Date.now();
    const result = await call("Runtime.evaluate", {
      expression: `(async()=>{const {mount}=await import(${JSON.stringify(`${pageBase}/varta-host.js`)});const logs=[];const app=mount({onLog:(line)=>logs.push(line)});try{const result=await app.finished;return{status:result.status,logs:result.logs??logs,error:result.error??null};}catch(error){return{status:"failed",logs:error.logs??logs,error:String(error.message??error)};}})()`,
      awaitPromise: true, returnByValue: true,
    });
    assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
    return { ...result.result.value, elapsed: Date.now() - opened };
  });
  return { native, nativeLines, nativeElapsed, browser };
}
const program = async (file) => (await readFile(new URL(`../crates/varta-runtime/src/net_cases/v38/${file}`, import.meta.url), "utf8")).replace("__BASE__", base);
let base, pageBase;
try {
  base = await listen(network); pageBase = await listen(pages);
  const source = (await program("long_stream.ts"))
    .replace("__COUNT__", COUNT).replace("__SIZE__", SIZE).replace("__PAUSE__", PAUSE_MS).replace("__GAP__", GAP_MS);
  const long = await bothHosts("long", source, { deadlinePerTurnMs: WINDOW_MS });
  assert.deepEqual(long.nativeLines, expected, long.native.stderr);
  assert(long.nativeElapsed >= COUNT * PAUSE_MS + WINDOW_MS, "the native stream outlived several windows");
  const browser = long.browser;
  assert.equal(browser.status, "ok", JSON.stringify(browser));
  assert.deepEqual(browser.logs, long.nativeLines, "Chrome prints exactly the native lines");
  assert(browser.elapsed >= COUNT * PAUSE_MS + WINDOW_MS, "the browser stream outlived several windows");
  assert.equal(agents.length, 2);
  assert.equal(agents[0], null, "the native host sends no User-Agent");
  assert(agents[1] && !agents[1].includes("varta-test-sdk"), "Chrome sends its own User-Agent");
  assert.equal(codings[0], "identity", "the native host asks for the identity coding");
  assert.match(codings[1] ?? "", /gzip/, "Chrome received, decoded and hid a gzip body");
  // The burst, under the production budgets of both hosts.
  const burstSource = (await program("burst.ts")).replace("__COUNT__", BURST_COUNT).replace("__SIZE__", BURST_SIZE).replace("__CAP__", CAP);
  const bursts = await bothHosts("burst", burstSource);
  assert.deepEqual(bursts.nativeLines, burstExpected, bursts.native.stderr);
  assert.equal(bursts.browser.status, "ok", JSON.stringify(bursts.browser));
  assert.deepEqual(bursts.browser.logs, bursts.nativeLines, "Chrome prints exactly the native burst lines");
  console.log(`Browser streaming acceptance passed: ${streamed} bytes over ${COUNT} events (${COUNT * PAUSE_MS} ms, window ${WINDOW_MS} ms), stall failure, host-owned User-Agent and decoded compression print the same ${expected.length} lines natively and in Chrome; a ${burst}-byte burst of ${BURST_COUNT} events, read at most ${CAP} bytes at a time under the production budgets, prints the same ${burstExpected.length} lines`);
} finally {
  await Promise.all([network, pages].map((server) => new Promise((done) => { server.close(done); server.closeAllConnections(); })));
  await rm(root, { recursive: true, force: true });
}
