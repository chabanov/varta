// Object spread into dictionaries, Record<string, T> inference and number keys.
interface Point {
  x: number;
  y: number;
  label?: string;
}
interface Options {
  mode?: string;
  [key: string]: string | number | undefined;
}
function merge(base: Record<string, number>, extra: Record<string, number>): Record<string, number> {
  return { ...base, ...extra, total: 0 };
}
function fromRecord(): void {
  const point: Point = { y: 2, x: 1 };
  const loose: Record<string, number | string | undefined> = { ...point, z: 3 };
  console.log(JSON.stringify(loose));
  point.label = "here";
  const again: Record<string, number | string | undefined> = { label: "first", ...point };
  console.log(JSON.stringify(again) + " " + JSON.stringify(Object.keys(again)));
}
function options(): void {
  const defaults: Options = { mode: "fast", retries: 3 };
  const mode = "slow";
  const chosen: Options = { ...defaults, mode, "1": 1 };
  console.log(JSON.stringify(chosen) + " " + String(chosen.mode));
}
function keysOf<T>(d: Record<string, T>): string[] {
  return Object.keys(d);
}
function first<T>(d: { [key: string]: T }, key: string, fallback: T): T {
  const value = d[key];
  return value === undefined ? fallback : value;
}
function names(): Record<string, string> {
  const byId: Record<string, string> = {};
  byId[10] = "ten";
  byId[2] = "two";
  byId["x"] = "ex";
  const n = 1.5;
  byId[n] = "one and a half";
  byId[-0] = "zero";
  return byId;
}
function numbers(): void {
  const byId = names();
  console.log(JSON.stringify(byId));
  console.log(String(byId[2]) + " " + String(byId[1.5]) + " " + String(byId[0]) + " " + String(2 in byId) + " " + String(3 in byId));
  delete byId[10];
  console.log(JSON.stringify(byId));
  const tally: Record<string, number> = {};
  for (const id of [3, 1, 3]) {
    if (id !== undefined) {
      tally[id] = (tally[id] ?? 0) + 1;
    }
  }
  tally[1] += 10;
  console.log(JSON.stringify(tally) + " " + String(Object.hasOwn(tally, 3)));
}
export function main(): void {
  console.log(JSON.stringify(merge({ b: 1, a: 2, "5": 5 }, { a: 20, c: 30 })));
  fromRecord();
  options();
  console.log(JSON.stringify(keysOf({ b: 1, a: 2 })) + " " + JSON.stringify(keysOf(names())));
  console.log(first({ x: "ex", y: "why" }, "y", "none") + " " + String(first({ n: 1 }, "m", 0)));
  numbers();
}
