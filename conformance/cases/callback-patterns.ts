interface Item { name: string; tags: string[]; price?: number }
class Shop {
  total([a, b]: [number, number]): number { return a + b; }
  label({ name, price = 0 }: Item): string { return `${name}:${price}`; }
}
function sum([a, b]: [number, number] = [1, 2]): number { return a + b; }
function head({ tags: [first = "none", ...others] }: Item): string { return first + "+" + String(others.length); }
function callbacks(): void {
  const pairs: [string, number][] = [["a", 1], ["b", 2]];
  console.log(pairs.map(([k, v]) => k + String(v * 10)).join(","));
  console.log(pairs.filter(([, v]) => v > 1).length.toString());
  const totals = pairs.reduce((acc, [, v]) => acc + v, 0);
  console.log(String(totals));
  pairs.forEach(([k, v], i) => { console.log(`${i}:${k}=${v}`); });
  const m = new Map<string, number>([["x", 3], ["y", 4]]);
  console.log([...m.entries()].map(([k, v]) => `${k}${v}`).join(" "));
  console.log(Array.from(m).map(([k, v]) => k.repeat(v)).join("|"));
  const o = { p: 1, q: 2 };
  console.log(Object.entries(o).map(([k, v]) => `${k}=${v}`).join("&"));
  const items: Item[] = [{ name: "pen", tags: ["office", "blue"], price: 2 }, { name: "cup", tags: [] }];
  console.log(items.map(({ name, price = 0 }) => `${name}${price}`).join(","));
  console.log(items.map(head).join(" "));
  const shop = new Shop();
  console.log(String(shop.total([4, 5])) + " " + items.map((it) => shop.label(it)).join(","));
  console.log(String(sum()) + " " + String(sum([10, 20])));
  const helpers = { swap([a, b]: [string, string]): string { return b + a; } };
  console.log(helpers.swap(["1", "2"]));
  const nested = [[1, [2, 3]]] as [number, [number, number]][];
  console.log(nested.map(([a, [b, c]]) => a + b + c).join(""));
  const restParam = (...[first, second]: number[]): number => first + second;
  console.log(String(restParam(5, 6)));
}
function iteratorHelpers(): void {
  const m = new Map<string, number>([["x", 3], ["y", 4]]);
  console.log(m.entries().map(([k, v]) => k + String(v)).toArray().join(","));
  for (const [k, v] of m) { console.log(k + v); }
  const s = new Set<string>(["a", "b"]);
  console.log([...s.entries()].map(([a, b]) => a + b).join(""));
}
function more(): void {
  const xs = ["a", "b", "c"];
  console.log([...xs.entries()].map(([i, v]) => v + String(i)).join(","));
  console.log(xs.entries().map(([i, v]) => `${i}${v}`).toArray().join(""));
  const pairs: [string, number][] = [["b", 2], ["a", 1], ["c", 3]];
  console.log(pairs.toSorted(([a], [b]) => a < b ? -1 : 1).map(([k]) => k).join(""));
  console.log(String(pairs.find(([k]) => k === "c")?.[1]));
  console.log(String(pairs.some(([, v]) => v > 2)) + String(pairs.every(([, v]) => v > 0)));
  console.log(pairs.flatMap(([k, v]) => [k, String(v)]).join(""));
  const m = new Map(pairs);
  console.log(m.entries().filter(([, v]) => v !== 2).map(([k]) => k).toArray().join(""));
  const grouped = Object.groupBy([1, 2, 3, 4], (n) => (n % 2 === 0 ? "even" : "odd"));
  console.log(Object.entries(grouped).map(([k, v]) => `${k}:${(v ?? []).join("")}`).join(" "));
  const withDefault = pairs.map(([k, v = 0, ...rest]: [string, number]) => k + String(v) + String(rest.length));
  console.log(withDefault.join(","));
  const objs = [{ a: 1, b: { c: 2 } }];
  console.log(objs.map(({ a, b: { c } }) => a + c).join(""));
  const restObj = objs.map(({ a, ...others }) => String(a) + JSON.stringify(others));
  console.log(restObj.join(""));
  const triple: [number, string, boolean] = [1, "t", true];
  const [first, ...tail] = triple;
  console.log(String(first) + " " + JSON.stringify(tail) + " " + String(tail.length));
  const [, , ...last] = triple;
  console.log(JSON.stringify(last));
  const [, , , ...none] = triple;
  console.log(String(none.length));
}
export function main(): void {
  callbacks();
  iteratorHelpers();
  more();
}
