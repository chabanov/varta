// for...in visits own keys in own-key order and skips keys deleted before
// their visit; compound, logical and update assignments and spreads write
// dictionary entries in place or at their own-key position.
export function main(): void {
  const counts: Record<string, number> = { b: 1, a: 2, "3": 3, d: 4 };
  let visited = "";
  for (const key in counts) {
    visited = visited + key;
    if (key === "b") {
      delete counts["d"];
      counts["e"] = 5;
    }
  }
  console.log(visited);
  counts["a"] += 10;
  counts.b++;
  counts["c"] ??= 7;
  counts["a"] ??= 100;
  const merged: Record<string, number> = { ...counts, a: 0, f: 6 };
  console.log(JSON.stringify(counts) + " " + JSON.stringify(merged));
}
