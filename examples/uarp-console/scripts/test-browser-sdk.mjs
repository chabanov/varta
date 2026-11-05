// The real uarp-sdk client in the official browser host, compared with its
// native runs. Every SDK group of src/tests/sdk/ (unary, problem documents,
// Retry-After retries, abort, timeout, finite SSE) runs:
//   1. natively against the loopback mock API (src/tests/sdk-server/, itself
//      a Varta HTTP/1 application) as an application instance that records
//      its network completions (`varta dev --app --network-record`);
//   2. natively again with the mock stopped, replaying that log
//      (`--network-replay`, no socket opens);
//   3. in local Chrome from the program's own `varta build --target browser`
//      bundle, mounted with the same log as its `networkReplay`, so no
//      browser-side mock server exists.
// The browser run must print the native replay's lines (each group ends
// with `SUMMARY N passed, 0 failed`) and use exactly its fuel, host calls,
// microtasks, output bytes, turns and idle maintenance.
//
//   node scripts/test-browser-sdk.mjs <varta-executable> [group…]
//
// Real local Chrome through ../../scripts/browser-probe.mjs (temporary
// profile, VARTA_CHROME_BIN overrides the path). The mock is the only server
// the native recording reaches; the page server serves only the bundle.
import { createServer } from "node:http";
import { readFile, writeFile, rm, mkdtemp } from "node:fs/promises";
import { join, resolve, sep, dirname } from "node:path";
import { tmpdir } from "node:os";
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";
import { withBrowserPage } from "../../../scripts/browser-probe.mjs";

const GROUPS = ["sdk-unary", "sdk-problem", "sdk-retry", "sdk-abort", "sdk-timeout", "sdk-sse"];
// The placeholder origin of varta.sdk-test.json; a run grants the mock's
// exact origin instead (net.request ports are never wildcards).
const PLACEHOLDER = "http://127.0.0.1:1";
const LISTENING = /^varta listening: (http:\/\/127\.0\.0\.1:\d+)$/m;
const MOCK_STARTUP_MS = 120_000;
const RUN_TIMEOUT_MS = 120_000;
const POLL_MS = 50;
const SUMMARY = /^varta log: SUMMARY ([1-9]\d*) passed, 0 failed$/m;
const USAGE_FIELDS = ["hostCalls", "microtasks", "outputBytes", "fuelUsed", "turns", "maintenancePeriods", "maintenanceFuel"];

const [executableArgument, ...requested] = process.argv.slice(2);
if (!executableArgument) throw new Error("Usage: node scripts/test-browser-sdk.mjs <varta-executable> [group…]");
const executable = resolve(executableArgument);
const groups = requested.length ? requested : GROUPS;
const console_ = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const run = promisify(execFile);

const logLines = (stdout) => stdout.split("\n").filter((line) => line.startsWith("varta log: "));
// The browser host's usage fields from a native `--budget-report`.
function nativeUsage(stderr) {
  const turns = [...stderr.matchAll(/^varta budget: turn \d+ \w+: fuel \d+\/\d+, host calls (\d+)\/\d+, microtasks (\d+),/gm)];
  const total = /^varta budget: total: (\d+) turns, fuel (\d+), peak memory \d+\/\d+ bytes, output (\d+)\/\d+ bytes$/m.exec(stderr);
  const maintenance = /^varta budget: maintenance total: (\d+) idle periods, fuel (\d+)$/m.exec(stderr);
  assert(total && turns.length === Number(total[1]), stderr);
  return {
    hostCalls: turns.reduce((sum, turn) => sum + Number(turn[1]), 0),
    microtasks: turns.reduce((sum, turn) => sum + Number(turn[2]), 0),
    outputBytes: Number(total[3]), fuelUsed: Number(total[2]), turns: Number(total[1]),
    maintenancePeriods: Number(maintenance?.[1] ?? 0), maintenanceFuel: Number(maintenance?.[2] ?? 0),
  };
}

async function startMock() {
  const child = spawn(executable, ["dev", "--config", "varta.sdk-server.json"], { cwd: console_, stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  child.stderr.on("data", (chunk) => { stderr += chunk; });
  const until = Date.now() + MOCK_STARTUP_MS;
  while (Date.now() < until) {
    const origin = LISTENING.exec(stderr)?.[1];
    if (origin) return { child, origin };
    if (child.exitCode !== null) throw new Error(`the mock API exited: ${stderr}`);
    await new Promise((done) => setTimeout(done, POLL_MS));
  }
  child.kill();
  throw new Error(`the mock API did not listen: ${stderr}`);
}
async function stop(child) {
  if (child && child.exitCode === null && child.signalCode === null) {
    const exited = new Promise((done) => child.once("exit", done));
    child.kill();
    await exited;
  }
}

const id = `${process.pid}`;
// Ignored by examples/uarp-console/.gitignore (`varta.sdk-test.loopback-*`)
// and dist/.
const configName = `varta.sdk-test.loopback-${id}.json`;
const out = `dist/sdk-browser-${id}`;
const work = await mkdtemp(join(tmpdir(), "varta-browser-sdk-"));
let mock, pages;
const results = [];
try {
  mock = await startMock();
  const base = JSON.parse(await readFile(join(console_, "varta.sdk-test.json"), "utf8"));
  const urls = base.capabilities.net.request.urls;
  assert.deepEqual(urls, [`${PLACEHOLDER}/**`], "varta.sdk-test.json grants the placeholder origin");
  const config = { ...base, out: `${out}/sdk-test`, capabilities: { ...base.capabilities, net: { request: { ...base.capabilities.net.request, urls: [`${mock.origin}/**`] } } } };
  await writeFile(join(console_, configName), JSON.stringify(config, null, 2));
  const environment = (group) => ({ ...process.env, TEST_GROUP: group, SDK_BASE_URL: mock.origin });
  const dev = (group, ...extra) => run(executable, ["dev", "--app", "--budget-report", "--config", configName, ...extra], { cwd: console_, env: environment(group), maxBuffer: 1 << 24 })
    .catch((error) => { throw new Error(`${group} ${extra.join(" ")}: ${error.stderr ?? error}`); });
  // 1. Record every group against the mock.
  for (const group of groups) {
    const recorded = await dev(group, "--network-record", join(work, `${group}.json`));
    assert.match(recorded.stdout, SUMMARY, `${group}: native recording\n${recorded.stdout}${recorded.stderr}`);
  }
  await stop(mock.child);
  // 2. Replay natively with the mock stopped.
  const native = new Map();
  for (const group of groups) {
    const replayed = await dev(group, "--network-replay", join(work, `${group}.json`));
    assert.match(replayed.stdout, SUMMARY, `${group}: native replay\n${replayed.stdout}${replayed.stderr}`);
    native.set(group, { logs: logLines(replayed.stdout), usage: nativeUsage(replayed.stderr) });
  }
  // 3. Chrome: the program's own bundle, the log supplied by the embedder.
  await run(executable, ["build", "--target", "browser", "--config", configName], { cwd: console_, maxBuffer: 1 << 20 });
  const web = join(console_, out, "web");
  pages = createServer(async (request, response) => {
    try {
      const name = decodeURIComponent(new URL(request.url, "http://loopback").pathname);
      if (name === "/blank.html") { response.writeHead(200, { "content-type": "text/html" }).end("<!doctype html><title>sdk</title>"); return; }
      const path = resolve(web, `.${name}`);
      if (!path.startsWith(web + sep)) throw new Error("outside the bundle");
      const bytes = await readFile(path);
      response.writeHead(200, { "cache-control": "no-store", "content-type": path.endsWith(".js") ? "text/javascript" : "application/octet-stream" }).end(bytes);
    } catch { response.writeHead(404).end(); }
  });
  const pageBase = await new Promise((done) => pages.listen(0, "127.0.0.1", () => done(`http://127.0.0.1:${pages.address().port}`)));
  await withBrowserPage(`${pageBase}/blank.html`, async (call) => {
    const evaluate = async (expression) => {
      const result = await call("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
      assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
      return result.result.value;
    };
    for (const group of groups) {
      const log = await readFile(join(work, `${group}.json`), "utf8");
      const inputs = [["TEST_GROUP", group], ["SDK_BASE_URL", mock.origin]];
      await evaluate(`(async()=>{const {mount}=await import(${JSON.stringify(`${pageBase}/varta-host.js`)});
        const probe={logs:[],result:undefined,failure:undefined};(globalThis.sdkProbes??={})[${JSON.stringify(group)}]=probe;
        mount({networkReplay:${JSON.stringify(log)},environmentInputs:${JSON.stringify(inputs)},onLog:(line)=>probe.logs.push(line)}).finished
          .then((result)=>{if(result.status==='ok')probe.result={exitCode:result.exitCode,...Object.fromEntries(${JSON.stringify(USAGE_FIELDS)}.map((key)=>[key,result[key]]))};else probe.failure=result.error;},(error)=>{probe.failure=String(error.message??error);});
        return true;})()`);
      const until = Date.now() + RUN_TIMEOUT_MS;
      let probe;
      do {
        probe = await evaluate(`sdkProbes[${JSON.stringify(group)}]`);
        if (probe.result || probe.failure) break;
        await new Promise((done) => setTimeout(done, POLL_MS));
      } while (Date.now() < until);
      assert(probe.result || probe.failure, `${group}: the browser run did not end`);
      assert.equal(probe.failure, undefined, `${group}: ${probe.failure}\n${probe.logs.join("\n")}`);
      const expected = native.get(group);
      assert.deepEqual(probe.logs, expected.logs, `${group}: Chrome prints the native replay's lines`);
      assert.deepEqual(probe.result, { exitCode: 0, ...expected.usage }, `${group}: Chrome uses exactly the native replay's budgets`);
      const passed = SUMMARY.exec(expected.logs.join("\n"))[1];
      results.push(`${group} ${passed} passed`);
      console.log(`PASS ${group}: ${passed} tests, ${expected.usage.turns} turns, ${expected.usage.fuelUsed} fuel, as natively`);
    }
  });
  console.log(`Browser SDK acceptance passed (${results.join(", ")}): the real UarpClient in Chrome replays the native recordings with the native replay's lines and exact usage`);
} finally {
  await stop(mock?.child);
  pages?.close();
  await rm(join(console_, configName), { force: true });
  await rm(join(console_, out), { recursive: true, force: true });
  await rm(work, { recursive: true, force: true });
}
