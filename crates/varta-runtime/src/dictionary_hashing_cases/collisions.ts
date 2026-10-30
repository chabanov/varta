// V38 hashed dictionaries: probe chains. The pairs "ca9oj"/"cE8ua",
// "ca9oi"/"cE8ub" and "ca9oh"/"cE8uc" share their full 32-bit FNV-1a hash;
// "h31", "h97", "h158", ... share their home slot in tables of up to 32
// slots. Deleting inside a chain leaves a tombstone that later lookups probe
// past. Expected lines are Node 24's.
const SAME: string[] = ["ca9oj", "cE8ua", "ca9oi", "cE8ub", "ca9oh", "cE8uc"];
const SLOT: string[] = ["h31", "h97", "h158", "h161", "h217", "h239", "h266", "h288", "h330", "h345", "h363", "h392"];
export function main(): void {
  const d: Record<string, number> = {};
  for (let i = 0; i < SAME.length; i++) {
    d[SAME[i]] = i;
  }
  for (let i = 0; i < SLOT.length; i++) {
    d[SLOT[i]] = 100 + i;
  }
  let found = "";
  for (const key of SAME) {
    found += String(d[key]) + ",";
  }
  console.log(found + " " + String(d["ca9ok"]) + " " + String(d["h1"]));
  // Delete chain members in the middle; the rest stay reachable.
  delete d["ca9oj"];
  delete d["h158"];
  delete d["h239"];
  let after = "";
  for (const key of SLOT) {
    const value = d[key];
    after += value === undefined ? "-" : String(value - 100);
  }
  console.log(after + " " + String(d["cE8ua"]) + " " + String("ca9oj" in d));
  // Re-insert deleted chain members and add more colliding keys.
  d["h158"] = 1;
  d["ca9oj"] = 2;
  for (let round = 0; round < 4; round++) {
    for (const key of SLOT) {
      delete d[key];
    }
    for (const key of SLOT) {
      d[key] = round;
    }
  }
  console.log(Object.keys(d).join(","));
  let total = 0;
  for (const key of Object.keys(d)) {
    total += d[key] ?? 0;
  }
  console.log(String(total) + " " + String(Object.keys(d).length));
}
