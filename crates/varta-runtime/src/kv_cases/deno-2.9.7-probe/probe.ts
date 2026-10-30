/**
 * Synthetic V8 serializer probe for Varta's KV codec (test tooling; not run
 * in CI). Writes edge-case values with Deno KV, then prints each row's key
 * name, v_encoding and value bytes in hex: rows.txt is its output under the
 * Deno version in deno-version.txt.
 * Usage: deno run -A --unstable-kv probe.ts <fresh.kv> > rows.txt
 * `datediff` is a heap number (an integral double V8 writes as `N`); it is
 * 1500 unless a clock millisecond passes between its two reads.
 */
import { DatabaseSync } from "node:sqlite";
const path = Deno.args[0];
const kv = await Deno.openKv(path);
const two = "Привіт";
const shared = { s: 1 };
const values: [string, unknown][] = [
  ["smi30", 2 ** 30], ["smi30m1", 2 ** 30 - 1], ["smi31m1", 2 ** 31 - 1], ["negsmi30", -(2 ** 30)],
  ["negsmi30m1", -(2 ** 30) - 1], ["negsmi31", -(2 ** 31)], ["computed", 0.5 * 4],
  ["datediff", Date.now() - (Date.now() - 1500)],
  ["pad1", { a: two }], ["pad2", { ab: two }], ["pad3", [two, two]], ["padkey", { [two]: 1 }],
  ["bigkeys", { "4294967294": 1, "4294967295": 2, "2147483648": 3, "1073741824": 4 }],
  ["shared", { x: shared, y: shared }], ["bigneg", -5n], ["bigzero", 0n], ["bigbig", 2n ** 64n],
  ["lone", "a\ud800b"], ["latin1two", "é" + two.slice(0, 0)], ["arrprops", Object.assign([1, 2], { k: 1 })],
  ["holeend", [1, , ]], ["nested_bytes", [new Uint8Array([1]), new Uint8Array([2])]],
  ["sub", new Uint8Array(new Uint8Array([1, 2, 3, 4]).buffer, 1, 2)],
  ["emptyobjarr", [{}, []]], ["undefarr", [undefined, null]], ["neg0arr", [-0, NaN]],
  ["newarray", (() => { const a = new Array(2); a[0] = 1; a[1] = 2; return a; })()],
  ["pushed", (() => { const a: number[] = []; a.push(1); a.push(2); return a; })()],
];
for (const [k, v] of values) await kv.set(["p", k], v);
kv.close();
const db = new DatabaseSync(path);
for (const row of db.prepare("select k, v, v_encoding from kv order by k").all()) {
  const k = new Uint8Array(row.k as Uint8Array);
  const v = new Uint8Array(row.v as Uint8Array);
  const hex = (b: Uint8Array) => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
  console.log(new TextDecoder().decode(k.slice(4, -1)), row.v_encoding, hex(v));
}
