interface Point {
  x: number;
}
class Tag {
  constructor(public name: string) {}
}

function keyList(m: Map<number, string>): string {
  const out: string[] = [];
  for (const k of m.keys()) out.push(Object.is(k, -0) ? "-0" : String(k));
  return out.join(",");
}

export function main(): void {
  // SameValueZero number keys: every NaN alike, -0 stored and found as +0.
  const numbers = new Map<number, string>();
  numbers.set(NaN, "nan");
  numbers.set(0 / 0, "nan2");
  numbers.set(-0, "zero");
  numbers.set(0, "zero2");
  numbers.set(1.5, "x");
  numbers.set(Infinity, "inf");
  numbers.set(-Infinity, "-inf");
  numbers.set(2 ** 53, "big");
  numbers.set(-1e-300, "tiny");
  console.log(`${numbers.size} ${numbers.get(NaN)} ${numbers.get(Number("x"))} ${numbers.get(-0)} ${numbers.get(0)}`);
  console.log(`${keyList(numbers)} ${numbers.has(1.5)} ${numbers.has(1.25)} ${numbers.get(2 ** 53 + 1)} ${numbers.get(-1e-300)}`);
  console.log(`${numbers.delete(-0)} ${numbers.has(0)} ${numbers.delete(NaN)} ${numbers.has(NaN)} ${numbers.size}`);
  const zeros = new Set<number>([-0]);
  zeros.add(0);
  for (const z of zeros) console.log(`${Object.is(z, -0)} ${Object.is(z, 0)} ${zeros.size}`);

  // Booleans, bigints and strings.
  const flags = new Set<boolean>([true, false, true]);
  console.log(`${flags.size} ${flags.has(false)} ${[...flags].join(",")}`);
  const bigs = new Map<bigint, number>();
  bigs.set(10n, 1);
  bigs.set(5n + 5n, 2);
  bigs.set(-10n, 3);
  bigs.set(2n ** 70n, 4);
  bigs.set(2n ** 70n + 1n - 1n, 5);
  bigs.set(0n, 6);
  console.log(`${bigs.size} ${bigs.get(10n)} ${bigs.get(-10n)} ${bigs.get(2n ** 70n)} ${bigs.get(-0n)} ${bigs.has(11n)}`);
  const strings = new Set<string>(["", "a", "\u{1F600}", "\uD800", "a"]);
  console.log(`${strings.size} ${strings.has("")} ${strings.has("\uD800")} ${strings.has("\uDC00")} ${strings.has("\u{1F600}")}`);

  // References compare by identity.
  const p: Point = { x: 1 };
  const q: Point = { x: 1 };
  const points = new Map<Point, string>([[p, "p"], [q, "q"]]);
  points.set(p, "p2");
  console.log(`${points.size} ${points.get(p)} ${points.get(q)} ${points.has({ x: 1 })}`);
  const tags = new Set<Tag>();
  const t1 = new Tag("a");
  tags.add(t1).add(new Tag("a")).add(t1);
  const arrays = new Map<number[], number>();
  const xs = [1, 2];
  arrays.set(xs, 1);
  arrays.set([1, 2], 2);
  const fns = new Set<() => number>();
  const f = (): number => 1;
  fns.add(f);
  fns.add(f);
  fns.add((): number => 1);
  console.log(`${tags.size} ${tags.has(t1)} ${arrays.get(xs)} ${arrays.size} ${fns.size} ${fns.has(f)}`);

  // Deletion and re-insertion move a key to the end.
  const order = new Map<string, number>([["a", 1], ["b", 2], ["c", 3]]);
  order.delete("a");
  order.set("a", 4);
  order.set("b", 5);
  const pairs: string[] = [];
  for (const [k, v] of order) pairs.push(`${k}${v}`);
  console.log(pairs.join(" "));

  // Insertion and deletion during iteration, with rebuilds while it runs.
  const live = new Map<number, number>();
  for (let i = 0; i < 40; i++) live.set(i, i);
  const visited: number[] = [];
  for (const [k] of live) {
    visited.push(k);
    if (k === 0) {
      for (let i = 1; i < 36; i++) live.delete(i);
      for (let i = 100; i < 140; i++) live.set(i, i);
    }
    if (k === 100) live.delete(101);
    if (k === 139) live.set(200, 200);
  }
  console.log(`${visited.length} ${visited.slice(0, 6).join(",")} ${visited.slice(-3).join(",")} ${live.size}`);

  // Clear during iteration, then additions are visited.
  const cleared = new Set<string>(["x", "y", "z"]);
  const seen: string[] = [];
  cleared.forEach((v) => {
    seen.push(v);
    if (v === "x") {
      cleared.clear();
      cleared.add("w");
    }
  });
  console.log(`${seen.join("")} ${cleared.size}`);

  // A queue: take the first key, delete it and append another.
  const queue = new Map<string, number>();
  for (let i = 0; i < 50; i++) queue.set(`q${i}`, i);
  let taken = 0;
  let sum = 0;
  for (let round = 0; round < 3000; round++) {
    let first = "";
    for (const k of queue.keys()) {
      first = k;
      break;
    }
    sum += queue.get(first) ?? 0;
    queue.delete(first);
    queue.set(`r${round}`, round);
    taken++;
  }
  const firstKeys: string[] = [];
  for (const k of queue.keys()) {
    firstKeys.push(k);
    if (firstKeys.length === 3) break;
  }
  console.log(`${taken} ${sum} ${queue.size} ${firstKeys.join(",")}`);

  // Growth past many table rebuilds, deletions of most entries, reinsertion.
  const big = new Map<string, number>();
  for (let i = 0; i < 5000; i++) big.set(`key${i}`, i);
  for (let i = 0; i < 5000; i++) if (i % 5 !== 0) big.delete(`key${i}`);
  for (let i = 0; i < 5000; i += 7) big.set(`key${i}`, -i);
  let total = 0;
  big.forEach((v) => {
    total += v;
  });
  const head: string[] = [];
  for (const k of big.keys()) {
    head.push(k);
    if (head.length === 4) break;
  }
  console.log(`${big.size} ${total} ${head.join(",")} ${big.get("key35")} ${big.get("key7")} ${big.has("key1")}`);
  const set = new Set<number>();
  for (let i = 0; i < 4000; i++) set.add(i * 0.5);
  for (let i = 0; i < 4000; i++) if (i % 3 !== 0) set.delete(i * 0.5);
  console.log(`${set.size} ${set.has(1.5)} ${set.has(0.5)} ${[...set].slice(0, 4).join(",")}`);

  // size, instanceof, copies.
  const copy = new Map(order);
  copy.set("z", 0);
  const setCopy = new Set(strings);
  setCopy.delete("");
  console.log(`${order.size} ${copy.size} ${strings.size} ${setCopy.size} ${order instanceof Map} ${strings instanceof Set}`);
  const entries = Array.from(copy);
  console.log(`${entries.length} ${entries[3][0]} ${[...copy.values()].join(",")}`);
}
