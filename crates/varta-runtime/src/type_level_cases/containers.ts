// Class subtyping in containers: arrays, tuples, records, Map/Set values
// and unions accept subclass instances; `B[]` converts to `A[]`.
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
  name(): string {
    return "C";
  }
}
class Holder {
  items: A[] = [];
  add(b: B): void {
    this.items.push(b);
  }
}
function names(list: A[]): string {
  return list.map((a) => a.name()).join("");
}

export function main(): void {
  const xs: A[] = [];
  xs.push(new B());
  xs.push(new A());
  const literal: A[] = [new B(), new A()];
  const pair: [A, A] = [new B(), new A()];
  const record: { a: A } = { a: new B() };
  literal[1] = new B();
  console.log(`${names(xs)} ${names(literal)} ${pair[0].name()}${record.a.name()}`);
  const map = new Map<string, A>();
  map.set("k", new B());
  const set = new Set<A>();
  set.add(new C());
  let maybe: A | null = null;
  maybe = new C();
  console.log(`${map.get("k")!.name()} ${set.size} ${maybe ? maybe.name() : "-"}`);
  const bs: B[] = [new B()];
  const widened: A[] = bs;
  const mapped: A[] = bs.map((b) => b);
  console.log(`${names(bs)}${names(widened)}${names(mapped)}`);
  const holder = new Holder();
  holder.add(new B());
  const mixed = [new B(), new C()];
  const lists: Map<string, A[]> = new Map();
  lists.set("k", [new B(), new C()]);
  const pairs: [string, A][] = [["b", new B()]];
  console.log(`${names(holder.items)} ${names(mixed)} ${names(lists.get("k")!)} ${pairs[0][1].name()}`);
}
