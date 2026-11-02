import { test } from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { readFile } from "node:fs/promises";
import { registry } from "../../../varta-common/browser/registry.mjs";
const source = (await Promise.all(["scheduler.js", "timers.js", "network-policy.js", "network.js"].map((name) => readFile(new URL(name, import.meta.url), "utf8")))).join("\n");
const context = vm.createContext({ registry, URL, TextEncoder, Uint8Array, setTimeout, clearTimeout });
vm.runInContext(source + "\nglobalThis.policy=networkPolicy;globalThis.make=createNetwork;globalThis.scheduler=createScheduler;globalThis.timers=createTimers;", context);
const grant = (extra = {}) => ({ id: "net.request", patterns: ["https://api.test/v1/**", "http://127.0.0.1:8080/**"], methods: ["GET", "POST"], ...extra });
const policy = (requested = grant(), ceiling) => context.policy({ capabilities: requested ? [requested] : [] }, ceiling);
const descriptor = (url = "https://api.test/v1/a", { method = "GET", headers = [], flag = "0", body = "" } = {}) => [registry.network.version, method, url, String(headers.length), ...headers.flat(), flag, body].join("\n");
test("network scope matches the native scheme/authority/path/method cases", () => {
  const p = policy();
  for (const [url, method] of [["https://api.test/v1/items?x=1", "GET"], ["https://API.test:443/v1/items", "POST"], ["http://127.0.0.1:8080/", "GET"]]) assert.equal(p.allows(url, method), true, url);
  for (const [url, method] of [["https://api.test/v1/items", "DELETE"], ["https://api.test/v1/items", "get"], ["http://api.test/v1/items", "GET"], ["https://api.test:8443/v1/items", "GET"], ["https://api.test/v2/items", "GET"], ["https://api.test/v1/../admin", "GET"], ["https://api.test/v1/%2e%2e/admin", "GET"], ["https://user:pw@api.test/v1/x", "GET"], ["https://api.test/v1/x#frag", "GET"], ["https://api.test.evil/v1/x", "GET"], ["http://127.0.0.1:8081/", "GET"], ["ftp://api.test/v1/x", "GET"], ["/v1/relative", "GET"]]) assert.equal(p.allows(url, method), false, url);
});
test("empty, optional and namespace-confused ceilings deny; scopes and visible headers intersect", () => {
  for (const p of [policy(null), policy(grant(), {}), policy(grant(), { "net.fetch": {} }), policy(grant({ required: false })), policy(grant({ methods: [] }))]) assert.equal(p.allows("https://api.test/v1/a", "GET"), false);
  const p = policy(grant({ responseHeaders: ["content-type", "x-id"] }), { "net.request": { patterns: ["https://api.test/v1/a"], methods: ["GET"], responseHeaders: ["x-id"] } });
  assert.equal(p.allows("https://api.test/v1/a", "GET"), true);
  assert.equal(p.allows("https://api.test/v1/b", "GET"), false);
  assert.equal(p.allows("https://api.test/v1/a", "POST"), false);
  assert.deepEqual([...p.visibleHeaders()], ["x-id"]);
});
test("runtime scopes are bounded snapshots with literal query marks and bounded glob work", () => {
  const value = { patterns: ["https://api.test/v1/a?x=*"], methods: ["GET"] };
  const p = policy(grant(), { "net.request": value }); value.patterns[0] = "https://evil.test/**"; value.methods[0] = "POST";
  assert.equal(p.allows("https://api.test/v1/a?x=1", "GET"), true);
  assert.equal(p.allows("https://api.test/v1/abx=1", "GET"), false);
  assert.equal(p.wildcard("*".repeat(512), "a".repeat(8192)), false);
  for (const bad of [null, [], {}, { patterns: ["https://*.test/**"] }, { patterns: ["https://api.test/%20"] }, { patterns: ["https://api.test/../a"] }, { patterns: ["https://api.test/**"], methods: ["get"] }, { patterns: ["https://api.test/**"], methods: ["TRACE"] }, { patterns: [], responseHeaders: ["set-cookie"] }, { patterns: [], responseHeaders: ["X-Id"] }, { patterns: [], responseHeaders: ["x", "x"] }, { patterns: [], enabled: true }]) assert.throws(() => policy(grant(), { "net.request": bad }), /invalid network permission ceiling/);
});
function state(p = policy()) {
  const logs = [], starts = [], results = [], heads = [];
  const scheduler = context.scheduler(registry.profiles.at(-1), String);
  const network = context.make(scheduler, p, (text) => logs.push(text), (text) => text, (text) => { results.push(text); return text; }, (plan) => {
    starts.push(plan); let finish;
    const head = new Promise((resolve) => { finish = resolve; }); heads.push(finish);
    return { head, pull: async () => ({ kind: registry.network.completion.end, text: "" }), cancel() {} };
  });
  const op = network.implementations;
  const watch = (id, token) => { const serial = scheduler.implementations.promise_react(0, token); op.net_request_watch(serial, id); };
  const drain = (callback) => scheduler.drain({ exports: { [registry.scheduler.resumeExport](token) { callback(token); return 1; } } }, () => {});
  return { op, network, scheduler, logs, starts, heads, results, watch, drain };
}
test("denied and invalid requests finish without starting a transport", () => {
  const s = state(policy(grant(), {})), id = s.op.net_request_start_v2(descriptor(), 0);
  s.watch(id, 1n); assert.equal(s.network.alive(), false); s.network.pump(); assert.equal(s.starts.length, 0);
  s.drain(() => { assert.equal(s.op.net_request_take(id), registry.network.completion.denied); assert.match(s.op.net_request_text(id), /capability denied: net.request/); });
  assert.equal(s.logs.length, 1);
  assert.throws(() => s.op.net_request_text(id), /invalid network request operation/);
  const t = state(), invalid = t.op.net_request_start_v2(descriptor("https://api.test/v1/a", { headers: [["Host", "evil.test"]] }), 0);
  assert.equal(t.op.net_request_take(invalid), registry.network.completion.failed); assert.match(t.op.net_request_text(invalid), /set by the runtime/); t.network.pump(); assert.equal(t.starts.length, 0);
});
test("head and body events wake one waiter only on explicit delivery", async () => {
  const s = state(), id = s.op.net_request_start_v2(descriptor(), 0); s.watch(id, 1n);
  assert.equal(s.network.alive(), true); s.network.pump(); assert.equal(s.starts.length, 1);
  s.heads[0]({ kind: registry.network.completion.head, text: "head" }); await Promise.resolve();
  const seen = []; s.drain((token) => seen.push(token)); assert.deepEqual(seen, []);
  assert.equal(s.network.deliver(), true); s.drain((token) => seen.push(token)); assert.deepEqual(seen, [1n]);
  assert.equal(s.op.net_request_take(id), registry.network.completion.head); assert.equal(s.op.net_request_text(id), "head");
  s.op.net_body_read_bytes(id); s.watch(id, 2n); s.network.pump(); await Promise.resolve(); s.network.deliver(); s.drain(() => {});
  assert.equal(s.op.net_request_take(id), registry.network.completion.end); assert.equal(s.op.net_request_text(id), ""); assert.equal(s.network.alive(), false);
});
test("abort replaces queued completions and releases idle requests; late transfer results are ignored", async () => {
  const s = state(), id = s.op.net_request_start_v2(descriptor(), 0); s.watch(id, 1n); s.network.pump();
  s.heads[0]({ kind: registry.network.completion.head, text: "stale" }); await Promise.resolve();
  s.op.net_request_abort(id); s.drain(() => {});
  assert.equal(s.op.net_request_take(id), registry.network.completion.aborted); assert.equal(s.op.net_request_text(id), ""); assert.equal(s.network.deliver(), false);
  s.op.net_request_abort(id); assert.throws(() => s.op.net_request_take(id), /invalid network request operation/);
  const deferred = s.op.net_request_start_v2(descriptor(), 0); s.op.net_request_abort(deferred); s.op.net_request_take(deferred); s.network.pump(); assert.equal(s.starts.length, 1);
});
test("descriptor versions, byte bodies and pending slots fail closed before effects", () => {
  const s = state();
  for (const text of ["bad", descriptor().replace(registry.network.version, "varta-net-request-1"), descriptor().replace("\n0\n0\n", "\n999999999\n0\n"), descriptor(undefined, { flag: "2", body: "\u0100" })]) assert.throws(() => s.op.net_request_start_v2(text, 0), /malformed network request descriptor/);
  assert.deepEqual([...s.network.parse(descriptor(undefined, { method: "POST", flag: "2", body: "\u0000\u00ff" })).body], [0, 255]);
  for (let i = 0; i < registry.network.maxPending; i++) s.op.net_request_start_v2(descriptor(), 0);
  assert.throws(() => s.op.net_request_start_v2(descriptor(), 0), /pending network requests/); assert.equal(s.starts.length, 0); s.network.close(); s.network.pump(); assert.equal(s.starts.length, 0);
});

test("empty query delimiters remain part of an exact network scope",()=>{
  const p=policy(grant({patterns:["https://api.test/v1/a"]}));
  assert.equal(p.allows("https://api.test/v1/a","GET"),true);
  assert.equal(p.allows("https://api.test/v1/a?","GET"),false);
});

test("network watchers cannot reuse a continuation owned by a timer",()=>{
  const s=state(),t=context.timers(registry.profiles.at(-1),s.scheduler,()=>true,()=>{},String);
  const serial=s.scheduler.implementations.promise_react(0,1n);t.implementations.timer_start_v2(serial,1,0);
  const id=s.op.net_request_start_v2(descriptor(),0);assert.throws(()=>s.op.net_request_watch(serial,id),/serial/);assert.equal(s.starts.length,0);s.network.close();
});
