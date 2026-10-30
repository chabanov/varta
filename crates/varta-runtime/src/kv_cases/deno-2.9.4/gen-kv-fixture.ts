/**
 * Synthetic Deno KV fixture for Varta's kv capability (2026-10-30).
 * No tenant data: every key and value below is written here.
 * Usage: deno run -A --unstable-kv gen-kv-fixture.ts <out.kv> <manifest.json>
 * Writes the file, then a manifest naming each key, what was written and
 * the versionstamp Deno returned, so a reader can compare key by key.
 */
const [OUT, MANIFEST] = Deno.args;
const kv = await Deno.openKv(OUT);
const manifest: { key: Deno.KvKey; shape: string; note?: string; versionstamp?: string }[] = [];

async function put(key: Deno.KvKey, value: unknown, shape: string, opts?: { expireIn?: number }) {
  const r = await kv.set(key, value, opts);
  manifest.push({ key, shape, versionstamp: r.versionstamp, ...(opts?.expireIn ? { note: `expireIn ${opts.expireIn}` } : {}) });
}

// Keys: strings and numbers, as UARP uses them.
await put(["fixture", "key", "string"], "k", "string");
await put(["fixture", "key", 0], "k", "string");
await put(["fixture", "key", -1], "k", "string");
await put(["fixture", "key", 1.5], "k", "string");
await put(["fixture", "key", 2 ** 53], "k", "string");
await put(["fixture", "key", "ünïcødé-ключ-🔑"], "k", "string");
await put(["tenant", "t-1", "run", "r-1"], "k", "string");

// Strings: one-byte (Latin-1) and two-byte V8 strings, empty, long.
await put(["fixture", "str", "ascii"], "hello", "string one-byte");
await put(["fixture", "str", "latin1"], "café ñ ü", "string one-byte latin1");
await put(["fixture", "str", "utf16"], "Привіт 🌍", "string two-byte");
await put(["fixture", "str", "empty"], "", "string empty");
await put(["fixture", "str", "long"], "x".repeat(60_000), "string 60000 one-byte (values are capped at 64 KiB)");
await put(["fixture", "str", "long_utf16"], "я".repeat(30_000), "string 30000 two-byte");

// Numbers: Smi, beyond Smi, negative, -0, NaN, Infinity, fractions.
for (const [name, n] of [["zero", 0], ["small", 42], ["neg", -7], ["smi_max", 2 ** 30 - 1], ["beyond_smi", 2 ** 31], ["int53", 2 ** 53 - 1], ["float", 3.14159], ["neg_zero", -0], ["nan", NaN], ["inf", Infinity], ["neg_inf", -Infinity]] as const) {
  await put(["fixture", "num", name], n, `number ${name}`);
}
await put(["fixture", "bigint"], 12345678901234567890n, "bigint");

// Booleans, null, undefined.
await put(["fixture", "bool", "true"], true, "boolean true");
await put(["fixture", "bool", "false"], false, "boolean false");
await put(["fixture", "null"], null, "null");
await put(["fixture", "undefined"], undefined, "undefined");

// Objects and arrays.
await put(["fixture", "obj", "empty"], {}, "object empty");
await put(["fixture", "obj", "flat"], { a: 1, b: "two", c: true, d: null }, "object flat");
await put(["fixture", "obj", "nested"], { a: { b: { c: [1, { d: "e" }] } }, list: [], o: {} }, "object nested");
await put(["fixture", "obj", "undefined_field"], { a: 1, b: undefined }, "object with undefined field");
await put(["fixture", "obj", "numeric_keys"], { "1": "one", "10": "ten", x: "x" }, "object with integer-like keys");
await put(["fixture", "arr", "empty"], [], "array empty");
await put(["fixture", "arr", "dense"], [1, "two", true, null, { a: 1 }, [2]], "array dense mixed");
// deno-lint-ignore no-sparse-arrays
await put(["fixture", "arr", "holes"], [1, , 3], "array with hole");

// Binary, Date, Map, Set.
await put(["fixture", "bytes"], new Uint8Array([0, 1, 2, 255]), "Uint8Array (stored as v_encoding 2)");
await put(["fixture", "bytes_in_obj"], { data: new Uint8Array([9, 8, 7]) }, "Uint8Array inside object (V8)");
await put(["fixture", "date"], new Date(Date.UTC(2026, 9, 30, 12, 0, 0, 123)), "Date");
await put(["fixture", "map"], new Map<unknown, unknown>([["a", 1], [2, "b"]]), "Map");
await put(["fixture", "set"], new Set(["a", 1]), "Set");

// A record shaped like a stored UARP run, with synthetic values only.
await put(["tenant", "t-1", "run", "r-2"], {
  run_id: "r-2", tenant_id: "t-1", agent_id: "a-1", status: "completed",
  input: { message: "hi" }, output: { response: "hello" },
  metrics: { input_tokens: 120, output_tokens: 34, total_cost_usd: 0.000123, duration_ms: 1500 },
  metadata: {}, step_seq: 3, artifacts: [], created_at: "2026-10-30T12:00:00.000Z",
  error_code: undefined,
}, "UARP-like run record");

// KvU64: plain set, then atomic sum.
await put(["fixture", "u64", "plain"], new Deno.KvU64(7n), "KvU64 (v_encoding 3)");
await kv.set(["fixture", "u64", "sum"], new Deno.KvU64(10n));
const sum = await kv.atomic().sum(["fixture", "u64", "sum"], 5n).commit();
manifest.push({ key: ["fixture", "u64", "sum"], shape: "KvU64 10 then atomic sum 5 = 15", versionstamp: sum.ok ? sum.versionstamp : undefined });

// expireIn: far in the future, so it is still present when read.
await put(["fixture", "expire"], "lives a day", "string", { expireIn: 86_400_000 });

// Overwrite bumps the versionstamp; delete leaves no row.
await kv.set(["fixture", "overwrite"], "first");
await put(["fixture", "overwrite"], "second", "string, overwritten once");
await kv.set(["fixture", "deleted"], "gone");
await kv.delete(["fixture", "deleted"]);
manifest.push({ key: ["fixture", "deleted"], shape: "deleted: no row" });

// atomic check + set: the check passes on the current versionstamp, a stale one fails.
const cur = await kv.get(["fixture", "overwrite"]);
const ok = await kv.atomic().check(cur).set(["fixture", "cas"], { by: "check-and-set" }).commit();
const stale = await kv.atomic().check({ key: ["fixture", "overwrite"], versionstamp: "00000000000000000000" }).set(["fixture", "cas_stale"], 1).commit();
manifest.push({ key: ["fixture", "cas"], shape: "object written by atomic check+set", versionstamp: ok.ok ? ok.versionstamp : undefined });
manifest.push({ key: ["fixture", "cas_stale"], shape: `atomic with stale check: ok=${stale.ok}, no row` });

kv.close();
const replacer = (_k: string, v: unknown) => typeof v === "bigint" ? `${v}n` : typeof v === "number" && !Number.isFinite(v) ? String(v) : v;
await Deno.writeTextFile(MANIFEST, JSON.stringify({ deno: Deno.version, written: manifest.length, entries: manifest }, replacer, 2) + "\n");
console.log(`wrote ${manifest.length} manifest entries`);
