// Node 24 console notation (WHATWG Console "Formatter", Node util.format and
// util.inspect defaults), checked profile v38 (`node_console`).
class Point {
  x: number;
  y: number;
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }
}
interface Item { name: string; next: Item | null }
export function main(): void {
  console.log("total", 3, true, 10n, -0, null, undefined);
  console.log({ id: 7, tags: ["a", "it's"], nested: { a: { b: { c: 1 } } } });
  console.log(new Point(1, 2), [new Point(3, 4)]);
  console.log(new Map([["k", [1, 2]]]), new Set(["x"]), new Date(0));
  console.log("%s is %d%% (%i, %f) %j %o", "progress", 42.5, "7.9", "2.5", { a: 1 }, [1]);
  console.log(new TypeError("bad"), new Error("failed", { cause: "disk" }));
  const xs: number[] = [];
  for (let i = 0; i < 12; i++) xs.push(i * 11);
  console.log(xs);
  const first: Item = { name: "first", next: null };
  first.next = { name: "second", next: first };
  console.log(first);
}
