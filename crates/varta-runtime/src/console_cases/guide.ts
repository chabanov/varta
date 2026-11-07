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
  console.log("total", 3, true, 10n, -0); // total 3 true 10n -0
  console.log({ id: 7, tags: ["a", "b"], ok: false }); // { id: 7, tags: [ 'a', 'b' ], ok: false }
  console.log(new Point(1, 2)); // Point { x: 1, y: 2 }
  console.log(new Map([["k", [1, 2]]]), new Set(["x"])); // Map(1) { 'k' => [ 1, 2 ] } Set(1) { 'x' }
  console.log({ a: { b: { c: { d: 1 } } } }); // { a: { b: { c: [Object] } } }
  console.log("%s is %d%%", "progress", 42.5); // progress is 42.5%
  console.log(new Error("failed", { cause: "disk" })); // [Error: failed] { [cause]: 'disk' }
  const first: Item = { name: "first", next: null };
  first.next = { name: "second", next: first };
  console.log(first);
  // <ref *1> {
  //   name: 'first',
  //   next: { name: 'second', next: [Circular *1] }
  // }
}
