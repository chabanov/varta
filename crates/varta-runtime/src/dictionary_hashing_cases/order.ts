// V38 hashed dictionaries: own-key order (array-index keys ascending, then
// insertion order) through indexing, deletions, re-insertions, tombstones,
// resizes and out-of-order array-index keys. Expected lines are Node 24's.
function show(d: Record<string, number>): string {
  return JSON.stringify(Object.keys(d)) + " " + JSON.stringify(Object.values(d));
}
export function main(): void {
  const d: Record<string, number> = {};
  // Past the linear bound: the first keyed access indexes the dictionary.
  for (let i = 0; i < 20; i++) {
    d["k" + String(i)] = i;
  }
  console.log(show(d));
  // Array-index keys enumerate first, ascending, whenever they arrive.
  d["7"] = 107;
  d["3"] = 103;
  d["10"] = 110;
  d["01"] = 1001;
  d["4294967295"] = 4294967295;
  d["4294967294"] = 4294967294;
  console.log(show(d));
  // Deletions leave the rest in order; a re-insertion goes to the end.
  delete d["k3"];
  delete d["k0"];
  delete d["7"];
  delete d["missing"];
  d["k3"] = 33;
  d["7"] = 77;
  console.log(show(d));
  console.log(
    String("k3" in d) + " " + String("k0" in d) + " " + String(Object.hasOwn(d, "7")) + " " +
      String(d["k0"] === undefined) + " " + String(d["k19"]),
  );
  // Many deletions then insertions: tombstones, compaction and growth.
  for (let i = 0; i < 20; i += 2) {
    delete d["k" + String(i)];
  }
  for (let i = 100; i < 160; i++) {
    d["k" + String(i)] = i;
  }
  for (let i = 100; i < 160; i += 3) {
    delete d["k" + String(i)];
  }
  let sum = 0;
  let count = 0;
  for (const key in d) {
    const value = d[key];
    if (value !== undefined) {
      sum += value;
      count++;
    }
  }
  console.log(String(count) + " " + String(sum) + " " + Object.keys(d).slice(0, 8).join(","));
  console.log(JSON.stringify(d).length);
  // Replacing values keeps positions.
  d["k1"] = -1;
  d["3"] = -3;
  console.log(Object.keys(d).slice(0, 6).join(",") + " " + String(d["k1"]) + " " + String(d["3"]));
  // A dictionary emptied by deletions and refilled.
  const e: Record<string, string> = {};
  for (let round = 0; round < 3; round++) {
    for (let i = 0; i < 30; i++) {
      e["e" + String(i)] = String(round);
    }
    for (let i = 0; i < 30; i++) {
      delete e["e" + String(i)];
    }
  }
  e["last"] = "x";
  e["2"] = "y";
  console.log(JSON.stringify(e) + " " + String(Object.keys(e).length));
  // Descending array-index keys take the ordered path.
  const n: Record<string, boolean> = {};
  for (let i = 30; i > 0; i--) {
    n[String(i)] = i % 2 === 0;
  }
  n["x"] = true;
  n["0"] = false;
  console.log(Object.keys(n).join(","));
  const copy: Record<string, boolean> = { ...n, y: false };
  delete copy["15"];
  console.log(String(Object.keys(copy).length) + " " + Object.keys(copy).slice(-3).join(","));
}
