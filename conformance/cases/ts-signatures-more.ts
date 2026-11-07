// Overloaded call signatures and function intersections, generic call
// signatures, `this` in object types and pattern template literal types.
type Overloaded = { (x: string): string; (x: number): number };
type Both = ((x: string) => string) & ((x: number) => number);
type Identity = <T>(value: T) => T;
interface Mapper {
  <T>(items: T[]): T[];
}
interface Chain {
  value: number;
  step(): this;
}
type Tree = { label: string; parent: this | null };
type Handler = `on${string}`;
type Loud = Uppercase<string>;

export function main(): void {
  const echo = ((x: string | number) => x) as Overloaded;
  const both = ((x: string | number) => x) as Both;
  const text: string = echo("a");
  const value: number = both(2);
  console.log(`${text} ${value}`);
  const id: Identity = (item) => item;
  const map: Mapper = (items) => items;
  const n: number = id(1);
  console.log(`${n + 1} ${id("s")} ${map([1, 2]).length}`);
  const root: Tree = { label: "root", parent: null };
  const leaf: Tree = { label: "leaf", parent: root };
  const chain: Chain = { value: 1, step: () => chain };
  console.log(`${leaf.parent ? leaf.parent.label : "-"} ${chain.step().value}`);
  const handler: Handler = "onClick";
  const loud: Loud = "X";
  console.log(handler + loud);
}
