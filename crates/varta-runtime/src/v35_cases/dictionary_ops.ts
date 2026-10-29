// for...in over dictionaries and records; compound, logical and update assignments on entries.
interface Point {
  x: number;
  y: number;
  label?: string;
}
interface Stats {
  total: number;
  name?: string;
  [key: string]: number | string | undefined;
}
function iterate(): void {
  const counts: Record<string, number> = { b: 1, a: 2, "3": 3 };
  let text = "";
  for (const key in counts) {
    text = text + key + "=" + String(counts[key]) + ";";
  }
  console.log(text);
  let current: string = "none";
  for (current in counts) {
    if (current === "b") {
      break;
    }
  }
  console.log(current);
}
function arithmetic(): void {
  const counts: Record<string, number> = { b: 1, a: 2, "3": 3 };
  counts["a"] += 10;
  counts.b *= 5;
  counts["c"] ??= 7;
  counts["a"] ??= 100;
  console.log(JSON.stringify(counts));
}
function steps(): void {
  const counts: Record<string, number> = { b: 1, c: 7 };
  counts.b++;
  ++counts["c"];
  const before = counts["c"]--;
  console.log(JSON.stringify(counts) + " " + String(before));
}
function strings(): void {
  const words: Record<string, string> = {};
  words["greet"] ||= "hello";
  words["greet"] += " world";
  words["empty"] = "";
  words["empty"] ||= "filled";
  console.log(JSON.stringify(words));
}
function records(): void {
  const point: Point = { y: 2, x: 1 };
  let fields = "";
  for (const name in point) {
    fields = fields + name + ",";
  }
  point.label = "p";
  for (const name in point) {
    fields = fields + name + ";";
  }
  console.log(fields);
}
function mutation(): void {
  const seen: Record<string, number> = { a: 1, b: 2, c: 3, d: 4 };
  let visited = "";
  for (const key in seen) {
    visited = visited + key;
    if (key === "a") {
      delete seen["c"];
      seen["e"] = 5;
    }
  }
  console.log(visited + " " + JSON.stringify(seen));
}
function mixed(): void {
  const stats: Stats = { total: 1, name: "s", extra: 3 };
  stats.total += 4;
  stats.total++;
  stats["extra"] = "three";
  let keys = "";
  for (const k in stats) {
    keys = keys + k + ":" + String(stats[k]) + " ";
  }
  console.log(keys + String(stats.total));
}
export function main(): void {
  iterate();
  arithmetic();
  steps();
  strings();
  records();
  mutation();
  mixed();
}
