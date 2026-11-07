// Step 3 admissions: `infer` in intersection patterns, `this` in generic
// declarations (and generic declarations referring to themselves),
// `asserts this` methods and overloads with rest parameters.
type Both<T> = T extends { a: infer A } & { b: infer B } ? [A, B] : never;
type Twice<T> = T extends { a: infer X } & { b: infer X } ? X : never;

interface Chain<T> {
  value: T;
  set(v: T): this;
}
interface Link<T> {
  value: T;
  next: Link<T> | null;
}
type Tree<T> = { label: T; children: Tree<T>[] };
class Impl implements Chain<number> {
  value: number = 0;
  set(v: number): this {
    this.value = v;
    return this;
  }
}
function total(link: Link<number> | null): number {
  return link === null ? 0 : link.value + total(link.next);
}
function count(tree: Tree<string>): number {
  return tree.children.reduce((sum, child) => sum + count(child), 1);
}

class Shape {
  name: string = "shape";
  assertSquare(): asserts this is Square {
    if (!(this instanceof Square)) {
      throw new TypeError("not a square");
    }
  }
  assertReady(): asserts this {}
}
class Square extends Shape {
  side: number = 3;
}
function area(shape: Shape): number {
  shape.assertSquare();
  return shape.side * shape.side;
}

type Joiner = {
  (...parts: string[]): string;
  (n: number): string;
};
interface Sum {
  (first: number, ...rest: number[]): number;
  (text: string): number;
}

export function main(): void {
  const pair: Both<{ a: number; b: string; c: boolean }> = [1, "x"];
  const either: Twice<{ a: "p"; b: "q" }> = "q";
  console.log(`${pair[0]} ${pair[1]} ${either}`);
  const chain: Chain<number> = new Impl();
  const list: Link<number> = { value: 1, next: { value: 2, next: null } };
  const tree: Tree<string> = { label: "r", children: [{ label: "a", children: [] }] };
  console.log(`${chain.set(4).value} ${total(list)} ${count(tree)}`);
  const square = new Square();
  square.assertReady();
  console.log(`${area(square)}`);
  try {
    area(new Shape());
  } catch (error) {
    console.log(error instanceof TypeError ? error.message : "other");
  }
  const join = ((...parts: (string | number)[]) => parts.join("-")) as Joiner;
  const sum = ((...values: (number | string)[]) =>
    values.reduce((all: number, v) => all + (typeof v === "number" ? v : v.length), 0)) as Sum;
  console.log(`${join("a", "b", "c")} ${join(3)} ${join()} ${sum(1, 2, 3)} ${sum("four")}`);
}
