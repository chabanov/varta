interface P { a: number; b?: number; c: { d: string; e: number[] } }
function f({ a, b = 10, c: { d, e: [first, ...others] } }: P): string {
  return `${a} ${b} ${d} ${first} ${others.join("/")}`;
}
function g([x, y = 5]: number[]): number { return x + y; }
const pts: { x: number; y: number }[] = [{ x: 1, y: 2 }, { x: 3, y: 4 }];
export function main(): void {
  console.log(f({ a: 1, c: { d: "q", e: [1, 2, 3] } }));
  console.log(String(g([1])));
  const { a, ...rest } = { a: 1, b: 2, c: 3 };
  console.log(String(a) + " " + JSON.stringify(rest));
  let p = 1, q = 2;
  [p, q] = [q, p];
  console.log(`${p} ${q}`);
  const pairs: [string, number][] = [["x", 1], ["y", 2]];
  for (const [k, v] of pairs) { console.log(k + v); }
  const items: { n: number; m?: number }[] = [{ n: 1 }, { n: 2, m: 3 }];
  for (const { n, m = 0 } of items) { console.log(String(n + m)); }
  const [h, , j = 9, ...t] = [1, 2, undefined, 4, 5];
  console.log(`${h} ${j} ${t.join(",")}`);
  let r: number, s: number;
  ({ r, s } = { r: 7, s: 8 });
  console.log(String(r + s));
  console.log(pts.map(({ x, y }) => x * y).join(","));
  const tuple = ([one, two]: [number, string]): string => two + one;
  console.log(tuple([1, "z"]));
  const defaults = ({ dp = 1, dq }: { dp?: number; dq: number }): number => dp + dq;
  console.log(String(defaults({ dq: 2 })));
  let w = 0, z = 0, cs: number[] = [];
  const src = { xx: { yy: [1, 2, 3] } };
  ({ xx: { yy: [w, ...cs] } } = src);
  console.log(`${w} ${cs.join(",")}`);
  [w, z = 7] = [5];
  console.log(`${w} ${z}`);
  const obj = { k: 0 };
  [obj.k] = [9];
  console.log(String(obj.k));
}
