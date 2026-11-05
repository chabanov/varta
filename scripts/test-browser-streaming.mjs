// Real-Chrome acceptance of the v38 network contract for long-lived SDK
// streams and host-owned headers. One program
// (crates/varta-runtime/src/net_cases/v38/long_stream.ts) runs as a long-lived
// application instance natively (`varta dev --app`) and in local Chrome (its
// own `varta build --target browser` bundle, mounted) against the same
// loopback server (`net_cases/server.mjs` routes, with CORS for the page's
// other origin). Its event stream lasts several test-only per-turn deadlines
// (the inactivity window) with a chunk inside every window and carries more
// than 512 KiB; a stall longer than one window fails the read; an SDK
// User-Agent is ignored by both hosts. Both hosts must print the same lines.
// Nothing leaves 127.0.0.1.
// Usage: node scripts/test-browser-streaming.mjs [varta-executable]
import { createServer } from "node:http";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
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
const web = join(root, "dist", "web");
const pages = createServer(async (req, res) => {
  try {
    const name = decodeURIComponent(new URL(req.url, "http://loopback").pathname);
    if (name === "/blank.html") { res.writeHead(200, { "content-type": "text/html" }).end("<!doctype html><title>probe</title>"); return; }
    const path = resolve(web, `.${name}`);
    if (!path.startsWith(web + sep)) throw new Error("outside the bundle");
    const bytes = await readFile(path);
    res.writeHead(200, { "cache-control": "no-store", "content-type": path.endsWith(".js") ? "text/javascript" : path.endsWith(".html") ? "text/html" : "application/octet-stream" }).end(bytes);
  } catch { res.writeHead(404).end(); }
});
const listen = (server) => new Promise((done) => server.listen(0, "127.0.0.1", () => done(`http://127.0.0.1:${server.address().port}`)));
try {
  const base = await listen(network), pageBase = await listen(pages);
  const source = (await readFile(new URL("../crates/varta-runtime/src/net_cases/v38/long_stream.ts", import.meta.url), "utf8"))
    .replace("__BASE__", base).replace("__COUNT__", COUNT).replace("__SIZE__", SIZE).replace("__PAUSE__", PAUSE_MS).replace("__GAP__", GAP_MS);
  await writeFile(join(root, "main.ts"), source);
  await writeFile(join(root, "varta.config.json"), JSON.stringify({
    entry: "main.ts", target: "wasm", out: "dist/app", languageProfile: "checked-f64-utf16-v38",
    capabilities: { net: { request: { urls: [`${base}/**`], methods: ["GET"], responseHeaders: ["content-encoding", "content-length", "content-type"] } } },
    budgets: { deadlinePerTurnMs: WINDOW_MS },
  }));
  const started = Date.now();
  const native = await run(executable, ["dev", "--app"], { cwd: root, maxBuffer: 1 << 20 });
  const nativeLines = native.stdout.split("\n").filter((line) => line.startsWith("varta log:"));
  assert.deepEqual(nativeLines, expected, native.stderr);
  assert(Date.now() - started >= COUNT * PAUSE_MS + WINDOW_MS, "the native stream outlived several windows");
  await run(executable, ["build", "--target", "browser"], { cwd: root, maxBuffer: 1 << 20 });
  const browser = await withBrowserPage(`${pageBase}/blank.html`, async (call) => {
    const opened = Date.now();
    const result = await call("Runtime.evaluate", {
      expression: `(async()=>{const {mount}=await import(${JSON.stringify(`${pageBase}/varta-host.js`)});const logs=[];const app=mount({onLog:(line)=>logs.push(line)});try{const result=await app.finished;return{status:result.status,logs:result.logs??logs,error:result.error??null};}catch(error){return{status:"failed",logs:error.logs??logs,error:String(error.message??error)};}})()`,
      awaitPromise: true, returnByValue: true,
    });
    assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
    return { ...result.result.value, elapsed: Date.now() - opened };
  });
  assert.equal(browser.status, "ok", JSON.stringify(browser));
  assert.deepEqual(browser.logs, nativeLines, "Chrome prints exactly the native lines");
  assert(browser.elapsed >= COUNT * PAUSE_MS + WINDOW_MS, "the browser stream outlived several windows");
  assert.equal(agents.length, 2);
  assert.equal(agents[0], null, "the native host sends no User-Agent");
  assert(agents[1] && !agents[1].includes("varta-test-sdk"), "Chrome sends its own User-Agent");
  assert.equal(codings[0], "identity", "the native host asks for the identity coding");
  assert.match(codings[1] ?? "", /gzip/, "Chrome received, decoded and hid a gzip body");
  console.log(`Browser streaming acceptance passed: ${streamed} bytes over ${COUNT} events (${COUNT * PAUSE_MS} ms, window ${WINDOW_MS} ms), stall failure, host-owned User-Agent and decoded compression print the same ${expected.length} lines natively and in Chrome`);
} finally {
  await Promise.all([network, pages].map((server) => new Promise((done) => { server.close(done); server.closeAllConnections(); })));
  await rm(root, { recursive: true, force: true });
}
