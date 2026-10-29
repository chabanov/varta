interface Point {
  x: number;
  y: number;
}
class Base {
  id: number;
  constructor(id: number) {
    this.id = id;
  }
}
class Derived extends Base {}
function first(list: string[] | undefined): string {
  return list![0] ?? "empty";
}
export function main(): void {
  const point: Point = { x: 1, y: 2 };
  const values: unknown[] = ["text", point, new Derived(7), [1, 2, 3]];
  console.log((values[0] as string).length);
  console.log((values[1] as Point).y);
  const base = values[2] as Base;
  console.log((base as Derived).id);
  console.log((values[3] as number[]).length);
  console.log(first(["a"]));
  const level = "high" as "low" | "high";
  console.log(level);
}
