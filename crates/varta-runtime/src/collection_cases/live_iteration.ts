interface Node {
  name: string;
}
export function main(): void {
  // Live iteration: entries added during iteration are visited, deleted ones skipped.
  const s = new Set<number>([1, 2, 3]);
  const seen: number[] = [];
  for (const v of s) {
    seen.push(v);
    if (v === 1) {
      s.delete(2);
      s.add(4);
    }
    if (v === 4) s.add(5);
  }
  console.log(seen.join(","));
  // Re-adding a deleted key appends it again.
  const order = new Set<string>(["a", "b", "c"]);
  order.delete("a");
  order.add("a");
  console.log([...order].join(""));
  // Clear during iteration: later additions are still visited.
  const c = new Set<number>([1, 2]);
  const visited: number[] = [];
  for (const v of c) {
    visited.push(v);
    if (v === 1) {
      c.clear();
      c.add(9);
    }
  }
  console.log(visited.join(","));
  // Object identity keys.
  const a: Node = { name: "a" };
  const b: Node = { name: "a" };
  const nodes = new Map<Node, number>();
  nodes.set(a, 1);
  nodes.set(b, 2);
  nodes.set(a, 3);
  console.log(`${nodes.size} ${nodes.get(a)} ${nodes.get(b)} ${nodes.has({ name: "a" })}`);
  // Churn forces compaction of deleted entries.
  const churn = new Map<number, string>();
  for (let i = 0; i < 1100; i++) {
    churn.set(i, "v");
    if (i >= 3) churn.delete(i - 3);
  }
  console.log(`${churn.size} ${[...churn.keys()].join(",")} ${churn.get(1099)}`);
  // Compaction during iteration keeps the cursor.
  const live = new Set<number>();
  for (let i = 0; i < 1024; i++) live.add(i);
  for (let i = 0; i < 1014; i++) live.delete(i);
  let count = 0;
  let last = 0;
  for (const v of live) {
    count++;
    last = v;
    if (count === 2) {
      for (let i = 0; i < 100; i++) live.add(2000 + i);
    }
    if (count > 200) break;
  }
  console.log(`${count} ${last} ${live.size}`);
  // Boolean and bigint keys.
  const flags = new Map<boolean, string>([[true, "yes"], [false, "no"]]);
  console.log(`${flags.get(false)} ${flags.get(true)}`);
  const big2 = new Set<bigint>([1n, 2n, 1n]);
  console.log(`${big2.size} ${big2.has(2n)} ${big2.has(3n)}`);
  // Entries and values iterators.
  const m = new Map<string, number>([["x", 1], ["y", 2]]);
  for (const [k, v] of m.entries()) console.log(`${k}=${v}`);
  for (const v of m.values()) console.log(`${v}`);
  const e = new Set<string>(["p"]);
  for (const [x, y] of e.entries()) console.log(`${x}${y}`);
  e.forEach((value, key) => console.log(`${value}/${key}`));
  m.forEach((value, key) => console.log(`${key}->${value}`));
}
