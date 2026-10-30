// V38 hashed dictionaries at sizes past the v37 element limit, with garbage
// allocated between operations so that collections run while the vectors and
// tables grow, compact and rebuild (every live entry must survive them).
// Expected lines are Node 24's.
const N = 6000;
function churn(round: number): number {
  let garbage = 0;
  for (let i = 0; i < 8; i++) {
    const waste: string[] = [];
    for (let j = 0; j < 16; j++) {
      waste.push("w" + String(round * 100 + i * 16 + j));
    }
    garbage += waste.length;
  }
  return garbage;
}
export function main(): void {
  const d: Record<string, string> = {};
  let garbage = 0;
  for (let i = 0; i < N; i++) {
    d["key" + String(i)] = "v" + String(i);
    if (i % 500 === 0) {
      garbage += churn(i);
    }
  }
  for (let i = 0; i < N; i += 3) {
    delete d["key" + String(i)];
    if (i % 900 === 0) {
      garbage += churn(i);
    }
  }
  for (let i = 0; i < N; i += 6) {
    d["key" + String(i)] = "again" + String(i);
  }
  let present = 0;
  let checksum = 0;
  for (let i = 0; i < N; i++) {
    const value = d["key" + String(i)];
    if (value !== undefined) {
      present++;
      checksum = (checksum + value.length * (i % 97)) % 1000003;
    }
  }
  const keys = Object.keys(d);
  console.log(String(present) + " " + String(keys.length) + " " + String(checksum) + " " + String(garbage > 0));
  console.log(keys.slice(0, 3).join(",") + " " + keys.slice(-3).join(","));
  // Integer-like keys in reverse order past the linear bound.
  const ids: Record<string, number> = {};
  for (let i = 2000; i > 0; i--) {
    ids[String(i * 7)] = i;
  }
  for (let i = 1; i <= 2000; i += 2) {
    delete ids[String(i * 7)];
  }
  const idKeys = Object.keys(ids);
  console.log(String(idKeys.length) + " " + idKeys.slice(0, 3).join(",") + " " + String(ids["14000"]) + " " + String(ids["7"]));
}
