// Class arrays converted to arrays of an ancestor class are one array, as
// in JavaScript: writes through either name are seen through the other.
class A {
  x: number = 1;
  name(): string {
    return "A";
  }
}
class B extends A {
  y: number = 2;
  name(): string {
    return "B";
  }
}
class C extends A {
  z: number = 3;
  name(): string {
    return "C";
  }
}
function names(list: A[]): string {
  return list.map((a) => a.name()).join("");
}
function show(list: (B | C)[]): string {
  return list.map((v) => v.name()).join("");
}
function same(x: A[], y: A[]): boolean {
  return x === y;
}
function addB(list: A[]): void {
  list.push(new B());
}

// A second family whose arrays are also shared with a nullable union.
class Shape {
  area(): number {
    return 0;
  }
}
class Square extends Shape {
  side: number = 2;
  area(): number {
    return this.side * this.side;
  }
}
class Circle extends Shape {
  area(): number {
    return 3;
  }
}
type Shelf = { items: Shape[] };
function areas(list: (Shape | undefined)[]): string {
  return list.map((s) => (s === undefined ? "-" : `${s.area()}`)).join(",");
}

export function main(): void {
  const bs: B[] = [new B()];
  const all: A[] = bs;
  addB(all);
  console.log(`${bs.length} ${all.length} ${names(bs)} ${bs[1].y} ${same(bs, all)}`);

  const mixed = [new B(), new C(), new B()];
  const view: A[] = mixed;
  view.push(new C());
  console.log(`${names(view)} ${show(mixed)} ${mixed.length}`);
  const sorted = mixed.slice().sort((p, q) => (p.name() < q.name() ? -1 : 1));
  console.log(`${show(sorted)} ${show(mixed.filter((v) => v instanceof B))}`);
  const found = mixed.find((v) => v instanceof C);
  const popped = mixed.pop();
  console.log(`${found ? found.name() : "-"} ${popped ? popped.name() : "-"} ${view.length}`);
  const [p, q] = mixed;
  console.log(`${p === view[0]} ${q === view[1]} ${mixed.indexOf(q)} ${mixed.includes(p)}`);
  mixed.reverse();
  mixed.splice(0, 1, new C(), new B());
  console.log(`${names(view)} ${show([...mixed, new C()])} ${JSON.stringify(view)}`);
  mixed[0] = new B();
  mixed.fill(new C(), 3);
  console.log(`${names(view)} ${mixed.map((v) => (v instanceof B ? v.y : v.z)).join(",")}`);

  const pieces = [new Square(), new Circle()];
  const shapes: Shape[] = pieces;
  const loose: (Shape | undefined)[] = shapes;
  const shelf: Shelf = { items: shapes };
  shelf.items.push(new Square());
  loose.push(undefined);
  console.log(`${areas(loose)} ${pieces.length} ${shapes.includes(pieces[2])}`);
  loose.pop();
  for (const piece of pieces) {
    console.log(`${piece.area()}`);
  }
}
