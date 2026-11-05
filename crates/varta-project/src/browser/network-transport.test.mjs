import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";
import { registry } from "../../../varta-common/browser/registry.mjs";
const transportSource = await readFile(new URL("network-transport.js", import.meta.url), "utf8");
const context = vm.createContext({ registry, setTimeout, clearTimeout, AbortController, TextDecoder, Uint8Array, URL });
vm.runInContext(transportSource + "\nglobalThis.transport=browserTransport;", context);
const N = registry.network, C = N.completion;
const plan = (extra = {}) => ({ url: "https://api.test/a", method: "GET", headers: [], visible: new Set(["content-type", "x-id"]), ...extra });
function response(chunks = [], extra = {}) {
  let at = 0, cancelled = false;
  return { status: 200, statusText: "OK", type: "cors", headers: new Headers([["content-type", "text/plain"], ["x-id", "123"], ["x-secret", "hidden"]]),
    body: { getReader() { return { async read() { return at < chunks.length ? { value: Uint8Array.from(chunks[at++]), done: false } : { done: true }; }, async cancel() { cancelled = true; } }; } },
    get cancelled() { return cancelled; }, ...extra };
}
// The rows of one generated table of docs/embedding.md, first column only.
async function generated(block) {
  const text = await readFile(new URL("../../../../docs/embedding.md", import.meta.url), "utf8");
  const start = text.indexOf(`<!-- generated:${block}:begin`), end = text.indexOf(`<!-- generated:${block}:end`);
  assert(start >= 0 && end > start, block);
  return text.slice(start, end).split("\n").filter((line) => line.startsWith("| `")).map((line) => line.split("`")[1]);
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
  for (const name of ["Cookie", "Set-Cookie", "Origin", "Sec-Fetch-Site", "Proxy-Authorization", "X-HTTP-Method-Override"]) {
    const t = context.transport(plan({ headers: [[name, "x"]] }), async () => { effects++; return response(); });
    const head = await t.head; assert.equal(head.kind, C.failed); assert.match(head.text, /cannot set header/);
  }
  assert.equal(effects, 0);
});
test("host-owned headers (an SDK User-Agent) are ignored exactly as the native host ignores them", async () => {
  const owned = await generated("embedding-net-host-headers");
  assert.deepEqual(owned, ["accept-encoding", "user-agent"]);
  let sent;
  const headers = [["User-Agent", "uarp-sdk/1.0"], ["Accept", "application/json"], ["accept-encoding", "gzip"], ["Authorization", "Bearer k"]];
  const t = context.transport(plan({ headers }), async (_, options) => { sent = options.headers; return response(); });
  assert.equal((await t.head).kind, C.head);
  // The transport runs in its own realm: compare plain copies.
  assert.deepEqual(JSON.parse(JSON.stringify(sent)), [["Accept", "application/json"], ["Authorization", "Bearer k"]]);
  for (const [name] of sent) assert(!owned.includes(name.toLowerCase()), name);
});
test("opaque redirects never masquerade as native heads", async () => {
  const t = context.transport(plan(), async () => response([], { status: 0, type: "opaqueredirect" })), head = await t.head;
  assert.equal(head.kind, C.failed); assert.match(head.text, /browser transport cannot/);
});
test("Fetch-decoded bodies are admitted and hide their coding and coded length", async () => {
  const visible = new Set(["content-encoding", "content-length", "content-type", "x-id"]);
  const coded = response([[104, 105]], { headers: new Headers([["content-encoding", "gzip"], ["content-length", "22"], ["content-type", "text/plain"], ["x-id", "1"]]) });
  const t = context.transport(plan({ visible }), async () => coded);
  const head = await t.head;
  assert.equal(head.kind, C.head);
  assert.deepEqual(JSON.parse(head.text).headers, ["content-type", "text/plain", "x-id", "1"]);
  assert.equal((await t.pull(false)).text, "hi"); assert.equal((await t.pull(false)).kind, C.end);
  // An identity body keeps both, as on the native host.
  for (const encoding of [undefined, "identity", " Identity "]) {
    const fields = [["content-length", "2"], ["content-type", "text/plain"]];
    if (encoding !== undefined) fields.push(["content-encoding", encoding]);
    const u = context.transport(plan({ visible }), async () => response([[104, 105]], { headers: new Headers(fields) }));
    const shown = JSON.parse((await u.head).text).headers;
    assert(shown.includes("content-length"), String(encoding));
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
// The length of every chunk `transport` delivers in one read mode, to the end.
async function reads(transport, bytes) {
  await transport.head;
  const lengths = [];
  for (let chunk = await transport.pull(bytes); chunk.kind === C.chunk; chunk = await transport.pull(bytes)) lengths.push(bytes ? chunk.text.length : new TextEncoder().encode(chunk.text).length);
  return lengths;
}
test("one read delivers at most the per-read cap, splitting larger Fetch chunks like the native host", async () => {
  const cap = N.maxReadBytes;
  assert(cap > 0 && cap < N.maxChunkBytes);
  // A chunk of more than the old chunk bound, then one byte: the cap-sized
  // reads, the remainder, then the next chunk (the native read sequence).
  const split = Array.from({ length: Math.floor((N.maxChunkBytes + 3) / cap) }, () => cap);
  split.push((N.maxChunkBytes + 3) % cap, 1);
  for (const bytes of [true, false]) {
    const t = context.transport(plan(), async () => response([new Uint8Array(N.maxChunkBytes + 3).fill(65), [65]]));
    assert.deepEqual(await reads(t, bytes), split, String(bytes));
  }
  // Text reads count held-back bytes of a split sequence against the cap.
  const euro = new TextEncoder().encode("\u20ac".repeat(N.maxChunkBytes));
  const lengths = await reads(context.transport(plan(), async () => response([euro])), false);
  assert(lengths.every((length) => length <= cap), JSON.stringify(lengths));
  assert.equal(lengths.reduce((a, b) => a + b, 0), euro.length);
  // Byte reads put held-back bytes first, and buffered bytes after them
  // still count against the cap.
  const held = context.transport(plan(), async () => response([[0x61, 0xe2], new Uint8Array(2 * cap).fill(66)])); await held.head;
  assert.equal((await held.pull(false)).text, "a");
  assert.equal((await held.pull(true)).text, "\u00e2");
  assert.deepEqual(await reads(held, true), [cap, cap]);
});
test("body chunks stay bounded and the body limit applies per read, not per body", async () => {
  // A long stream of ordinary reads passes the old per-body total.
  const count = Math.floor(N.maxBodyBytes / N.maxChunkBytes) + 2;
  const long = context.transport(plan(), async () => response(Array.from({ length: count }, () => new Uint8Array(N.maxChunkBytes).fill(66)))); await long.head;
  let total = 0;
  for (let chunk = await long.pull(true); chunk.kind === C.chunk; chunk = await long.pull(true)) total += chunk.text.length;
  assert.equal(total, count * N.maxChunkBytes); assert(total > N.maxBodyBytes);
  // One read that receives more than the bound still fails.
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
test("the inactivity window bounds the head and each body wait, renews with every chunk and releases its timer", async () => {
  const timers = new Map(); let sequence = 0;
  const deadlineContext = vm.createContext({ registry, AbortController, TextDecoder, Uint8Array, URL,
    setTimeout(callback, delay) { const id = ++sequence; timers.set(id, { callback, delay }); return id; },
    clearTimeout(id) { timers.delete(id); } });
  vm.runInContext(transportSource + "\nglobalThis.transport=browserTransport;", deadlineContext);
  const window = 123, settle = () => new Promise((resolve) => setImmediate(resolve));
  const expire = () => { assert.equal(timers.size, 1); const [id, timer] = [...timers][0]; assert.equal(timer.delay, window); timers.delete(id); timer.callback(); };
  const pendingHead = deadlineContext.transport(plan({ timeoutMs: window }), async () => new Promise(() => {}));
  await settle(); expire();
  const timedOut = await pendingHead.head; assert.equal(timedOut.kind, C.failed); assert.equal(timedOut.text, N.errors.timeout); assert.equal(timers.size, 0);
  // Reads resolve only when the test delivers a chunk.
  const deliveries = []; let cancelled = false;
  const stream = deadlineContext.transport(plan({ timeoutMs: window }), async () => response([], { body: { getReader() { return {
    read() { return new Promise((resolve) => deliveries.push(resolve)); },
    async cancel() { cancelled = true; },
  }; } } }));
  assert.equal((await stream.head).kind, C.head);
  assert.equal(timers.size, 0, "no window runs while the guest reads nothing");
  for (let chunk = 0; chunk < 3; chunk++) {
    const read = stream.pull(true); await settle();
    assert.equal(timers.size, 1, "each wait gets a fresh window");
    deliveries.shift()({ value: Uint8Array.of(65 + chunk), done: false });
    assert.equal((await read).text, String.fromCharCode(65 + chunk)); assert.equal(timers.size, 0);
  }
  const stalled = stream.pull(true); await settle(); expire();
  const stall = await stalled; assert.equal(stall.kind, C.failed); assert.equal(stall.text, N.errors.bodyTimeout);
  assert.equal(cancelled, true); assert.equal(timers.size, 0);
  const complete = deadlineContext.transport(plan({ timeoutMs: window }), async () => response());
  await complete.head; assert.equal((await complete.pull(false)).kind, C.end); assert.equal(timers.size, 0);
  const closed = deadlineContext.transport(plan({ timeoutMs: window }), async () => response());
  await closed.head; closed.cancel(); assert.equal(timers.size, 0);
});
