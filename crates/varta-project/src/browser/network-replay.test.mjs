// The browser replay transport (net::replay): the same log reader limits as
// native, grant-filtered heads, strict log order, read modes and aborts, and
// createNetwork's abandon/close paths. Divergence texts come from the
// generated registry, so they equal the native runtime's.
import { test } from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { readFile } from "node:fs/promises";
import { registry } from "../../../varta-common/browser/registry.mjs";
const source = (await Promise.all(["scheduler.js", "timers.js", "network-policy.js", "network.js", "network-transport.js"].map((name) => readFile(new URL(name, import.meta.url), "utf8")))).join("\n");
const context = vm.createContext({ registry, URL, TextEncoder, TextDecoder, Uint8Array, setTimeout, clearTimeout, AbortController });
vm.runInContext(source + "\nglobalThis.snapshot=snapshotNetworkLog;globalThis.replay=replayTransport;globalThis.make=createNetwork;globalThis.policy=networkPolicy;globalThis.scheduler=createScheduler;", context);
const R = registry.network.replay, C = registry.network.completion, N = registry.network;
const URL_A = "http://127.0.0.1:8080/a", URL_B = "http://127.0.0.1:8080/b";
const head = (request, headers = ["content-type", "text/plain", "x-id", "7"]) => ({ kind: "head", step: 1, request, status: 200, statusText: "OK", location: "", headers });
const log = (events, requests = [{ method: "GET", url: URL_A }]) => JSON.stringify({ format: R.format, version: R.version, requests, events });
const plan = (url = URL_A, visible = ["content-type"]) => ({ method: "GET", url, headers: [], visible: new Set(visible) });
const plain = (value) => JSON.parse(JSON.stringify(value));
const descriptor = (url) => [N.version, "GET", url, "0", N.noBody, ""].join(N.separator);

test("the log reader refuses what the native reader refuses", () => {
  const ok = context.snapshot(log([head(0), { kind: "chunk", step: 1, request: 0, read: "bytes", text: "\u0000ÿ" }, { kind: "end", step: 1, request: 0 }]));
  assert.equal(ok.events.length, 3);
  assert.deepEqual(plain(context.snapshot(new TextEncoder().encode(log([])))), { format: R.format, version: R.version, requests: [{ method: "GET", url: URL_A }], events: [] });
  for (const [bad, reason] of [
    [log([{ kind: "chunk", step: 1, request: 0, read: "text", text: "x" }]), /order/],
    [log([head(0), { kind: "end", step: 0, request: 0 }]), /step/],
    [log([head(0, ["x-b", "1", "x-a", "2"])]), /headers/],
    [log([head(0, ["set-cookie", "a"])]), /headers/],
    [log([head(0, ["x-a", "a\r\nb"])]), /headers/],
    [log([head(0), { kind: "chunk", step: 1, request: 0, read: "bytes", text: "ā" }]), /chunk/],
    [log([head(0), { kind: "chunk", step: 1, request: 0, read: "text", text: "" }]), /chunk/],
    [log([head(0), { kind: "chunk", step: 1, request: 0, read: "text", text: "x".repeat(N.maxReadBytes + 1) }]), /chunk/],
    [log([{ kind: "abort", step: 1, request: 1 }]), /event/],
    [log([{ ...head(0), extra: 1 }]), /event/],
    [log([], [{ method: "TRACE", url: URL_A }]), /request/],
    [log([], [{ method: "GET", url: "HTTP://127.0.0.1:8080/a" }]), /request/],
    [JSON.stringify({ format: "other", version: R.version, requests: [], events: [] }), /format/],
    ["{", /no JSON/], [{}, /JSON text/], [" ".repeat(R.maxLogBytes + 1), /too large/],
  ]) assert.throws(() => context.snapshot(bad), reason);
});

test("replayed heads show what the replay grant shows, in log order with read modes", async () => {
  const events = [head(0), head(1), { kind: "chunk", step: 1, request: 0, read: "text", text: "é" }, { kind: "end", step: 1, request: 0 }, { kind: "failed", step: 2, request: 1, text: "request timed out" }];
  const r = context.replay(context.snapshot(log(events, [{ method: "GET", url: URL_A }, { method: "GET", url: URL_B }])));
  const a = r.transport(plan()), b = r.transport(plan(URL_B, ["x-id"]));
  assert.deepEqual(plain(await a.head), { kind: C.head, text: `{"headers":["content-type","text/plain"],"location":"","status":200,"statusText":"OK","url":"${URL_A}"}` });
  assert.deepEqual(plain(await b.head), { kind: C.head, text: `{"headers":["x-id","7"],"location":"","status":200,"statusText":"OK","url":"${URL_B}"}` });
  // B's failure waits behind A's chunk and end, in log order.
  let failed; b.pull(false).then((completion) => { failed = completion; });
  await Promise.resolve(); assert.equal(failed, undefined);
  assert.deepEqual(plain(await a.pull(false)), { kind: C.chunk, text: "é" });
  assert.deepEqual(plain(await a.pull(false)), { kind: C.end, text: "" });
  await Promise.resolve(); assert.deepEqual(plain(failed), { kind: C.failed, text: "request timed out" });
  r.finish(); assert.deepEqual(plain(r.position()), { event: 5, remaining: 0 });
});

test("divergences fail with the registry texts the native runtime uses", () => {
  const events = [head(0), { kind: "chunk", step: 1, request: 0, read: "text", text: "x" }, { kind: "abort", step: 1, request: 0 }];
  const r = context.replay(context.snapshot(log(events)));
  assert.throws(() => r.transport(plan(URL_B)), { message: `network replay diverged: request 0 is GET ${URL_B}, the log recorded GET ${URL_A}` });
  // A `$` in a URL is text, never a replacement pattern.
  assert.throws(() => r.transport(plan("http://127.0.0.1:8080/$&")), { message: `network replay diverged: request 0 is GET http://127.0.0.1:8080/$&, the log recorded GET ${URL_A}` });
  const t = r.transport(plan());
  assert.throws(() => t.pull(true), { message: "network replay diverged: request 0 read bytes, the log recorded text" });
  assert.throws(() => r.transport(plan()), { message: `network replay diverged: request GET ${URL_A} is not in the log` });
  assert.throws(() => r.finish(), { message: "network replay ended with 2 logged events undelivered" });
  const early = context.replay(context.snapshot(log([head(0), { kind: "end", step: 1, request: 0 }])));
  const transfer = early.transport(plan());
  assert.throws(() => transfer.cancel(), { message: "network replay diverged: request 0 was aborted where the log continues" });
  assert.equal(R.errors.ungranted.replace("{request}", "0").replace("{method}", "GET").replace("{url}", URL_A), `network replay refused: logged request 0 (GET ${URL_A}) is not granted`);
});

test("createNetwork replays after its own grant check, abandons unstarted aborts and closes without aborting", async () => {
  const events = [{ kind: "abort", step: 1, request: 0 }, head(1), { kind: "abort", step: 1, request: 1 }];
  const r = context.replay(context.snapshot(log(events, [{ method: "GET", url: URL_A }, { method: "GET", url: URL_B }])));
  const logs = [];
  const p = context.policy({ capabilities: [{ id: "net.request", patterns: ["http://127.0.0.1:8080/**"], methods: ["GET"] }] });
  const network = context.make(context.scheduler(registry.profiles.at(-1), String), p, (text) => logs.push(text), (text) => text, (text) => text,
    Object.assign((value) => r.transport(value), { abandon: (value) => r.abandon(value) }));
  const start = (url) => network.implementations.net_request_start_v2(descriptor(url), 0);
  // Started and aborted in one turn: the transport never ran, yet the log's
  // first request and its abort are consumed, as natively.
  network.implementations.net_request_abort(start(URL_A));
  const second = start(URL_B); network.pump();
  // A denied request never reaches the replay.
  const denied = start("http://127.0.0.1:9999/x");
  assert.equal(network.implementations.net_request_take(denied), C.denied);
  await Promise.resolve(); await Promise.resolve();
  assert.equal(network.deliver(), true);
  assert.equal(network.implementations.net_request_take(second), C.head);
  network.implementations.net_request_abort(second);
  r.finish();
  assert.match(logs[0], /capability denied/);
  // Closing an instance never counts as an abort.
  const quiet = context.replay(context.snapshot(log([head(0)])));
  const closing = context.make(context.scheduler(registry.profiles.at(-1), String), p, () => {}, (text) => text, (text) => text, (value) => quiet.transport(value));
  closing.implementations.net_request_start_v2(descriptor(URL_A), 0);
  closing.pump(); await Promise.resolve(); closing.close(); quiet.finish();
});
