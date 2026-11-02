import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";
import { registry } from "../../../varta-common/browser/registry.mjs";
const context = vm.createContext({ registry, setTimeout, clearTimeout, AbortController, TextDecoder, Uint8Array, URL });
vm.runInContext(await readFile(new URL("network-transport.js", import.meta.url), "utf8") + "\nglobalThis.transport=browserTransport;", context);
const N = registry.network, C = N.completion;
const plan = (extra = {}) => ({ url: "https://api.test/a", method: "GET", headers: [], visible: new Set(["content-type", "x-id"]), ...extra });
function response(chunks = [], extra = {}) {
  let at = 0, cancelled = false;
  return { status: 200, statusText: "OK", type: "cors", headers: new Headers([["content-type", "text/plain"], ["x-id", "123"], ["x-secret", "hidden"]]),
    body: { getReader() { return { async read() { return at < chunks.length ? { value: Uint8Array.from(chunks[at++]), done: false } : { done: true }; }, async cancel() { cancelled = true; } }; } },
    get cancelled() { return cancelled; }, ...extra };
}
test("transport forbids ambient credentials, redirect following and unrequested response headers", async () => {
  let options;
  const t = context.transport(plan(), async (url, value) => { assert.equal(url, "https://api.test/a"); options = value; return response(); });
  const head = await t.head;
  assert.equal(head.kind, C.head);
  assert.equal(head.text, '{"headers":["content-type","text/plain","x-id","123"],"location":"","status":200,"statusText":"OK","url":"https://api.test/a"}');
  assert.equal(options.credentials, "omit"); assert.equal(options.redirect, "manual"); assert.equal(options.mode, "cors"); assert.equal(options.referrerPolicy, "no-referrer"); assert.equal(options.cache, "no-store");
  t.cancel(); assert.equal(options.signal.aborted, true);
});
test("browser-owned headers fail before Fetch instead of silently disappearing", async () => {
  let effects = 0;
  for (const name of ["Cookie", "Set-Cookie", "Origin", "Accept-Encoding", "User-Agent", "Sec-Fetch-Site", "Proxy-Authorization", "X-HTTP-Method-Override"]) {
    const t = context.transport(plan({ headers: [[name, "x"]] }), async () => { effects++; return response(); });
    const head = await t.head; assert.equal(head.kind, C.failed); assert.match(head.text, /cannot set header/);
  }
  assert.equal(effects, 0);
});
test("opaque redirects and exposed compression never masquerade as native heads", async () => {
  for (const value of [response([], { status: 0, type: "opaqueredirect" }), response([], { headers: new Headers({ "content-encoding": "gzip" }) })]) {
    const t = context.transport(plan(), async () => value), head = await t.head;
    assert.equal(head.kind, C.failed); assert.match(head.text, /browser transport cannot/);
  }
});
test("UTF8 carry survives chunk boundaries, byte-mode transitions and BOMs", async () => {
  const t = context.transport(plan(), async () => response([[0xef, 0xbb, 0xbf, 0x61, 0xe2, 0x82], [0xac]]));
  await t.head;
  const a = await t.pull(false); assert.equal(a.kind, C.chunk); assert.equal(a.text, "\uFEFFa");
  assert.equal((await t.pull(true)).text, "\u00e2\u0082");
  assert.equal((await t.pull(true)).text, "\u00ac");
  assert.equal((await t.pull(false)).kind, C.end);
  const incomplete = context.transport(plan(), async () => response([[0x61, 0xe2, 0x82]])); await incomplete.head;
  assert.equal((await incomplete.pull(false)).text, "a"); assert.equal((await incomplete.pull(false)).text, "\ufffd"); assert.equal((await incomplete.pull(false)).kind, C.end);
});
test("body chunks and total decoded bytes remain bounded", async () => {
  const t = context.transport(plan(), async () => response([new Uint8Array(N.maxChunkBytes + 3).fill(65)])); await t.head;
  assert.equal((await t.pull(true)).text.length, N.maxChunkBytes); assert.equal((await t.pull(true)).text, "AAA"); assert.equal((await t.pull(true)).kind, C.end);
  const large = response([new Uint8Array(N.maxBodyBytes + 1)]), u = context.transport(plan(), async () => large); await u.head;
  const failure = await u.pull(true); assert.equal(failure.kind, C.failed); assert.equal(failure.text, N.errors.bodyLimit); assert.equal(large.cancelled, true);
});
test("native invalid-prefix decoding, visible-header limits and body failures stay explicit", async () => {
  const t = context.transport(plan(), async () => response([[0x61, 0xe0], [0xa0, 0x80]])); await t.head;
  assert.equal((await t.pull(false)).text, "a\ufffd"); assert.equal((await t.pull(false)).text, "\ufffd\ufffd");
  const fields = new Headers(Array.from({ length: N.maxResponseHeaders + 1 }, (_, i) => [`x-${i}`, "v"]));
  const h = context.transport(plan(), async () => response([], { headers: fields })); assert.equal((await h.head).kind, C.failed);
  const broken = context.transport(plan(), async () => response([], { body: { getReader() { return { async read() { throw new Error("closed"); }, async cancel() {} }; } } })); await broken.head;
  assert.match((await broken.pull(false)).text, /browser response body failed: closed/);
});
test("operation deadlines survive headers and abort pending bodies; completion and cancellation release timers", async () => {
  const timers = new Map(); let sequence = 0;
  const deadlineContext = vm.createContext({ registry, AbortController, TextDecoder, Uint8Array, URL,
    setTimeout(callback, delay) { const id = ++sequence; timers.set(id, { callback, delay }); return id; },
    clearTimeout(id) { timers.delete(id); } });
  vm.runInContext(await readFile(new URL("network-transport.js", import.meta.url), "utf8") + "\nglobalThis.transport=browserTransport;", deadlineContext);
  const expire = () => { assert.equal(timers.size, 1); const [id, timer] = [...timers][0]; assert.equal(timer.delay, 123); timers.delete(id); timer.callback(); };
  const pendingHead = deadlineContext.transport(plan({ timeoutMs: 123 }), async (_, { signal }) => new Promise((_, reject) => signal.addEventListener("abort", () => reject(new Error("deadline")), { once: true })));
  expire(); assert.match((await pendingHead.head).text, /browser request failed: deadline/); assert.equal(timers.size, 0);
  let cancelled = false, signal;
  const pendingBody = deadlineContext.transport(plan({ timeoutMs: 123 }), async (_, options) => {
    signal = options.signal;
    return response([], { body: { getReader() { return {
      read() { return new Promise((_, reject) => signal.addEventListener("abort", () => reject(new Error("deadline")), { once: true })); },
      async cancel() { cancelled = true; },
    }; } } });
  });
  assert.equal((await pendingBody.head).kind, C.head); assert.equal(timers.size, 1);
  const read = pendingBody.pull(true); expire();
  assert.match((await read).text, /browser response body failed: deadline/); assert.equal(cancelled, true); assert.equal(timers.size, 0);
  const complete = deadlineContext.transport(plan({ timeoutMs: 123 }), async () => response());
  await complete.head; assert.equal((await complete.pull(false)).kind, C.end); assert.equal(timers.size, 0);
  const closed = deadlineContext.transport(plan({ timeoutMs: 123 }), async () => response());
  await closed.head; closed.cancel(); assert.equal(timers.size, 0);
});
