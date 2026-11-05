// Recorded network replay acceptance (net::replay). A Varta HTTP service
// (`varta dev`, loopback only) serves JSON, a server-sent event stream, an
// error status and a 503 with Retry-After. A v38 view application records a
// scripted native session against it (`varta dev --view-script
// --network-record`): fetch completions, a later request aborted in the turn
// it started in (before the browser's pump creates any transfer), an abort
// of an open stream and a timer backoff; it then exits, so every host checks
// that the log was consumed. With the service stopped, the same log is
// replayed natively (`--network-replay`) and in local Chrome as the
// embedder-supplied `networkReplay` of the application's own `varta build
// --target browser` bundle (`mount`). Recording, native replay and Chrome
// must print the same log lines; native replay and Chrome must end with the
// same view snapshot (the #app tree, and its accessible text against
// Chrome's accessibility tree) and the same usage (fuel, host calls,
// microtasks, output bytes, turns). A log with one more logged request fails
// both replays with the same undelivered-events text. Chrome opens no
// connection during the replay: the page server records every request, and
// the stopped service could not answer one. Nothing leaves 127.0.0.1.
// Usage: node scripts/test-browser-replay.mjs [varta-executable]
import { createServer } from "node:http";
import { mkdir, mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { join, resolve, sep } from "node:path";
import { tmpdir } from "node:os";
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";
import assert from "node:assert/strict";
import { withBrowserPage } from "./browser-probe.mjs";
import { registry } from "../crates/varta-common/browser/registry.mjs";

const PROFILE = "checked-f64-utf16-v38";
// Seconds of the Retry-After backoff, and the script's virtual wait for it.
const RETRY_AFTER_S = 1, RETRY_WAIT_MS = RETRY_AFTER_S * 1000;
// Wall-clock bounds of the script's network waits and of the harness.
const NETWORK_WAIT_MS = 5000, READY_TIMEOUT_MS = 15000, POLL_MS = 20;
const executable = resolve(process.argv[2] ?? "target/debug/varta");
const run = promisify(execFile);
const root = await mkdtemp(join(tmpdir(), "varta-browser-replay-"));

const SERVICE = `
const EVENTS = 'id: 1\\ndata: one\\n\\nid: 2\\ndata: two\\n\\nid: 3\\ndata: three\\n\\n';
function handle(request: VartaHttpRequest): VartaHttpResponse {
  const text = (status: number, type: string, body: string): VartaHttpResponse =>
    ({ status, body, headers: [{ name: 'content-type', value: type }] });
  if (request.target === '/items') return text(200, 'application/json', '{"items":["alpha","beta"]}');
  if (request.target === '/events') return text(200, 'text/event-stream', EVENTS);
  if (request.target === '/flaky?attempt=1') {
    return { status: 503, body: 'busy', headers: [{ name: 'content-type', value: 'text/plain' }, { name: 'retry-after', value: '${RETRY_AFTER_S}' }] };
  }
  if (request.target === '/flaky?attempt=2') return text(200, 'text/plain', 'ready');
  return text(404, 'text/plain', 'nope');
}
export function main(): void {}
varta.http.serve({ hostname: '127.0.0.1', port: 0 }, handle);
`;

// One line per network turn that completes a step: one view commit per turn.
const CLIENT = `
const BASE = '__BASE__';
let label = 0;
let shown = '';
function show(text: string): void {
  shown = shown + text + '|';
  View.setText(label, shown);
  View.commit();
  console.log(text);
}
interface Items { items: string[] }
async function body(response: Response): Promise<string> {
  const stream = response.body;
  if (stream === null) {
    return '';
  }
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let text = '';
  for (;;) {
    const { done, value } = await reader.read();
    if (done) {
      return text;
    }
    text += decoder.decode(value);
  }
}
async function attempt(n: number): Promise<void> {
  const response = await fetch(BASE + '/flaky?attempt=' + String(n));
  const text = await response.text();
  const after = response.headers.get('retry-after');
  if (response.status === 503 && after !== null) {
    show('attempt ' + String(n) + ': 503 ' + text + ', retry after ' + after + ' s');
    setTimeout(() => { attempt(n + 1); }, Number(after) * 1000);
    return;
  }
  show('attempt ' + String(n) + ': ' + String(response.status) + ' ' + text);
  // The session ends here on every host, which checks the log was consumed.
  varta.process.exit(0);
}
async function run(): Promise<void> {
  // The later request is aborted in the turn both started in.
  const early = new AbortController();
  const listing = fetch(BASE + '/items');
  const dropped = fetch(BASE + '/events', { signal: early.signal });
  early.abort();
  let drop = 'resolved';
  try {
    await dropped;
  } catch (error) {
    drop = error instanceof DOMException ? error.name : 'other';
  }
  const listed = await listing;
  const items = JSON.parse(await listed.text()) as Items;
  show('items ' + String(listed.status) + ' ' + items.items.join(',') + ', early abort ' + drop);
  const events = await fetch(BASE + '/events');
  const names: string[] = [];
  const received = await body(events);
  for (const block of received.split('\\n\\n')) {
    for (const line of block.split('\\n')) {
      if (line.startsWith('data: ')) {
        names.push(line.slice(6));
      }
    }
  }
  show('events ' + String(events.status) + ' ' + (events.headers.get('content-type') ?? 'none') + ' ' + names.join(','));
  const controller = new AbortController();
  const held = await fetch(BASE + '/events', { signal: controller.signal });
  const stream = held.body;
  let outcome = 'no body';
  if (stream !== null) {
    const reader = stream.getReader();
    const first = await reader.read();
    controller.abort();
    try {
      await reader.read();
      outcome = 'read again';
    } catch (error) {
      outcome = (first.done ? 'nothing' : 'data') + ', then ' + (error instanceof DOMException ? error.name : 'other');
    }
  }
  show('aborted ' + outcome);
  const missing = await fetch(BASE + '/missing');
  const reason = await missing.text();
  show('missing ' + String(missing.status) + ' ' + reason);
  await attempt(1);
}
export function main(): void {
  const root = View.node();
  View.createElement(root, 1);
  View.setAttribute(root, 1, 'app');
  label = View.node();
  View.createText(label, 'idle');
  View.insert(root, label, 0);
  View.mount(root);
  run();
}
`;
const SCRIPT = { inputs: [{ input: "wait_network", ms: NETWORK_WAIT_MS }, { input: "wait", ms: RETRY_WAIT_MS }, { input: "wait_network", ms: NETWORK_WAIT_MS }] };
const EXPECTED = [
  "varta log: items 200 alpha,beta, early abort AbortError",
  "varta log: events 200 text/event-stream one,two,three",
  "varta log: aborted data, then AbortError",
  "varta log: missing 404 nope",
  `varta log: attempt 1: 503 busy, retry after ${RETRY_AFTER_S} s`,
  "varta log: attempt 2: 200 ready",
];

async function project(name, source, capabilities) {
  const directory = join(root, name);
  await mkdir(directory);
  await writeFile(join(directory, "main.ts"), source);
  await writeFile(join(directory, "varta.config.json"), JSON.stringify({ entry: "main.ts", target: "wasm", out: "dist/app", languageProfile: PROFILE, capabilities }));
  return directory;
}
// The service's listening address, from its readiness line.
function serve(directory) {
  const child = spawn(executable, ["dev"], { cwd: directory, stdio: ["ignore", "ignore", "pipe"] });
  return new Promise((done, fail) => {
    let text = "";
    const timer = setTimeout(() => { child.kill(); fail(new Error(`service not ready: ${text}`)); }, READY_TIMEOUT_MS);
    child.stderr.on("data", (data) => {
      text += data;
      const match = /varta listening: (http:\/\/127\.0\.0\.1:\d+)/.exec(text);
      if (match) { clearTimeout(timer); done({ child, base: match[1] }); }
    });
    child.on("exit", () => { clearTimeout(timer); fail(new Error(`service exited: ${text}`)); });
  });
}
const logLines = (stdout) => stdout.split("\n").filter((line) => line.startsWith("varta log:"));
// The native view snapshot printed after the log lines.
function viewSnapshot(stdout) {
  const start = stdout.indexOf("\n{") + 1, end = stdout.lastIndexOf("}\n") + 1;
  return JSON.parse(stdout.slice(start, end));
}
// The #app subtree of a native snapshot: tags, attributes and texts.
function nativeApp(snapshot) {
  const find = (node) => node.attributes?.id === "app" ? node : (node.children ?? []).map(find).find(Boolean);
  const shape = (node) => node.text !== undefined ? { text: node.text } : { tag: node.tag, attributes: node.attributes ?? {}, children: (node.children ?? []).map(shape) };
  return shape(find(snapshot.tree));
}
// The accessible texts of a native snapshot, in order.
function nativeTexts(snapshot) {
  const texts = (node) => [...(node.role === "text" ? [node.name] : []), ...(node.children ?? []).flatMap(texts)];
  return texts(snapshot.accessibility);
}
// The browser host's usage fields from the native budget report.
function nativeUsage(stderr) {
  const prefix = "varta budget: ";
  const turns = [...stderr.matchAll(/^varta budget: turn \d+ \w+: fuel (\d+)\/(\d+), host calls (\d+)\/\d+, microtasks (\d+),/gm)];
  const total = /^varta budget: total: (\d+) turns, fuel (\d+), peak memory \d+\/\d+ bytes, output (\d+)\/\d+ bytes$/m.exec(stderr);
  assert(total && turns.length === Number(total[1]) && stderr.includes(prefix), stderr);
  const last = turns.at(-1);
  return {
    fuelRemaining: Number(last[2]) - Number(last[1]), hostCalls: turns.reduce((sum, turn) => sum + Number(turn[3]), 0),
    microtasks: turns.reduce((sum, turn) => sum + Number(turn[4]), 0), outputBytes: Number(total[3]), fuelUsed: Number(total[2]), turns: Number(total[1]),
  };
}
const USAGE_FIELDS = ["fuelRemaining", "hostCalls", "microtasks", "outputBytes", "fuelUsed", "turns"];

let service, pages;
try {
  const server = await project("service", SERVICE, { net: { listen: ["127.0.0.1:0"] } });
  service = await serve(server);
  const client = await project("client", CLIENT.replace("__BASE__", service.base), {
    net: { request: { urls: [`${service.base}/**`], methods: ["GET"], responseHeaders: ["content-type", "retry-after"] } },
    time: { timer: true },
    process: { exit: true },
  });
  await writeFile(join(client, "script.json"), JSON.stringify(SCRIPT));
  const dev = (...extra) => run(executable, ["dev", "--view-script", "script.json", ...extra], { cwd: client, maxBuffer: 1 << 24 });
  const recorded = await dev("--network-record", "log.json");
  assert.deepEqual(logLines(recorded.stdout), EXPECTED, recorded.stderr);
  service.child.kill(); await new Promise((done) => service.child.once("exit", done)); service = undefined;
  const logText = await readFile(join(client, "log.json"), "utf8");
  const log = JSON.parse(logText);
  const R = registry.network.replay;
  assert.equal(log.format, R.format);
  const kinds = new Set(log.events.map((event) => event.kind));
  for (const kind of R.kinds) assert(kinds.has(kind) || kind === "failed", `the log covers ${kind}`);
  // The later request (index 1) was aborted before any completion arrived:
  // its abort is the log's first event, ahead of the first request's head.
  assert.deepEqual([log.events[0].kind, log.events[0].request], ["abort", 1], logText);
  const native = await dev("--network-replay", "log.json", "--budget-report");
  assert.deepEqual(logLines(native.stdout), EXPECTED, native.stderr);
  const snapshot = viewSnapshot(native.stdout);
  assert.deepEqual(snapshot, viewSnapshot(recorded.stdout));
  const app = nativeApp(snapshot), texts = nativeTexts(snapshot);
  const usage = nativeUsage(native.stderr);
  // A log holding one more logged request than the session makes fails
  // natively once the session ends.
  const longer = JSON.stringify({ ...log, requests: [...log.requests, log.requests[0]], events: [...log.events, { kind: "failed", step: log.events.at(-1).step, request: log.requests.length, text: "request timed out" }] });
  await writeFile(join(client, "longer.json"), longer);
  const unconsumed = R.errors.unconsumed.replace("{remaining}", "1");
  const refused = await dev("--network-replay", "longer.json").then(() => assert.fail("the longer log replayed"), (error) => error);
  assert(refused.stderr.includes(unconsumed), refused.stderr);
  // Chrome: the application's own bundle, the log supplied by the embedder.
  await run(executable, ["build", "--target", "browser"], { cwd: client, maxBuffer: 1 << 20 });
  const web = join(client, "dist", "web"), requested = [];
  pages = createServer(async (request, response) => {
    try {
      const name = decodeURIComponent(new URL(request.url, "http://loopback").pathname);
      requested.push(name);
      if (name === "/blank.html") { response.writeHead(200, { "content-type": "text/html" }).end("<!doctype html><title>replay</title>"); return; }
      const path = resolve(web, `.${name}`);
      if (!path.startsWith(web + sep)) throw new Error("outside the bundle");
      const bytes = await readFile(path);
      response.writeHead(200, { "cache-control": "no-store", "content-type": path.endsWith(".js") ? "text/javascript" : "application/octet-stream" }).end(bytes);
    } catch { response.writeHead(404).end(); }
  });
  const pageBase = await new Promise((done) => pages.listen(0, "127.0.0.1", () => done(`http://127.0.0.1:${pages.address().port}`)));
  const browser = await withBrowserPage(`${pageBase}/blank.html`, async (call) => {
    const evaluate = async (expression) => {
      const result = await call("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
      assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
      return result.result.value;
    };
    // Mount the bundle with a log and wait until the instance ends (the
    // guest's exit, or a replay failure): only then does the browser host
    // check that every logged event was delivered.
    const replay = async (name, text) => {
      await evaluate(`(async()=>{const {mount}=await import(${JSON.stringify(`${pageBase}/varta-host.js`)});
        const container=document.createElement('div');container.dataset.replay=${JSON.stringify(name)};document.body.append(container);
        const probe={logs:[],result:undefined,failure:undefined};(globalThis.replayProbes??={})[${JSON.stringify(name)}]=probe;
        mount({container,networkReplay:${JSON.stringify(text)},onLog:(line)=>probe.logs.push(line)}).finished
          .then((result)=>{probe.result=result.status==='ok'?{exitCode:result.exitCode,...Object.fromEntries(${JSON.stringify(USAGE_FIELDS)}.map((key)=>[key,result[key]]))}:undefined;probe.failure=result.status==='ok'?undefined:result.error;},(error)=>{probe.failure=String(error.message??error);});
        return true;})()`);
      const until = Date.now() + READY_TIMEOUT_MS;
      let probe;
      do {
        probe = await evaluate(`replayProbes[${JSON.stringify(name)}]`);
        if (probe.result || probe.failure) return probe;
        await new Promise((done) => setTimeout(done, POLL_MS));
      } while (Date.now() < until);
      assert.fail(`the ${name} replay did not end: ${JSON.stringify(probe)}`);
    };
    const probe = await replay("log", logText);
    assert.equal(probe.failure, undefined, probe.failure);
    // The #app tree of the replayed view, as the native snapshot's.
    const { root: document } = await call("DOM.getDocument", { depth: -1, pierce: true });
    const attributes = (node) => Object.fromEntries((node.attributes ?? []).flatMap((value, index, all) => index % 2 ? [] : [[value, all[index + 1]]]));
    const find = (node) => attributes(node).id === "app" ? node : [...(node.children ?? []), ...(node.shadowRoots ?? [])].map(find).find(Boolean);
    const TEXT_NODE = 3;
    const shape = (node) => node.nodeType === TEXT_NODE ? { text: node.nodeValue } : { tag: node.localName, attributes: attributes(node), children: (node.children ?? []).map(shape) };
    const element = find(document);
    assert(element, "the view mounted #app");
    // Chrome's accessibility tree below #app: its static texts, in order.
    // An ended instance's view is frozen inert, which hides it from the
    // accessibility tree; lift that on the probe page only, so the tree is
    // the one the view had when the guest exited.
    const frozen = (node) => Object.hasOwn(attributes(node), "inert") ? node : [...(node.children ?? []), ...(node.shadowRoots ?? [])].map(frozen).find(Boolean);
    const inert = frozen(document);
    assert(inert, "the ended view is inert");
    await call("DOM.removeAttribute", { nodeId: inert.nodeId, name: "inert" });
    await call("Accessibility.enable", {});
    const [top] = (await call("Accessibility.getPartialAXTree", { nodeId: element.nodeId, fetchRelatives: false })).nodes;
    const texts = async (node) => {
      const own = node.role?.value === "StaticText" ? [node.name?.value] : [];
      const { nodes } = await call("Accessibility.getChildAXNodes", { id: node.nodeId });
      for (const child of nodes) own.push(...await texts(child));
      return own;
    };
    const accessible = await texts(top);
    const longerProbe = await replay("longer", longer);
    return { ...probe, tree: shape(element), texts: accessible, longer: longerProbe };
  });
  assert.deepEqual(browser.logs, EXPECTED, "Chrome prints the native replay's lines");
  assert.deepEqual(browser.tree, app, "Chrome ends with the native replay's #app tree");
  assert.deepEqual(browser.texts, texts, "Chrome's accessible texts are the native snapshot's");
  assert.deepEqual(browser.result, { exitCode: 0, ...usage }, "Chrome exits with the native replay's usage");
  assert.equal(browser.longer.failure, unconsumed, "Chrome refuses the longer log as natively");
  // The page origin served only the probe page and the bundle (the browser
  // may ask for its icon); a replay request to the stopped service would
  // have failed and changed the lines above.
  assert.deepEqual(requested.filter((name) => !["/blank.html", "/favicon.ico", "/varta-host.js"].includes(name) && !name.startsWith("/app.") && !name.startsWith("/varta-")), [], "the replay fetched nothing but the bundle");
  console.log(`Browser replay acceptance passed: a ${log.events.length}-event log of ${log.requests.length} requests recorded from a Varta loopback service (JSON, a later request aborted in its start turn, server-sent events, an abort, a 404 and a Retry-After backoff) replays natively and in Chrome with the same ${EXPECTED.length} lines, #app tree, accessible texts and usage (${usage.turns} turns, ${usage.fuelUsed} fuel, ${usage.hostCalls} host calls, ${usage.microtasks} microtasks), and a log with an undelivered request fails both`);
} finally {
  service?.child.kill();
  if (pages) await new Promise((done) => { pages.close(done); pages.closeAllConnections(); });
  await rm(root, { recursive: true, force: true });
}
