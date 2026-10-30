// Generic classes (v37 `class_generics`): one class per type-argument list,
// generic constructors with explicit, contextual and inferred arguments,
// generic bases, declared subclasses of instances, static members of the
// template, generic methods of generic classes and instances in fields,
// unions, `unknown` and JSON.
class Page<T> {
  items: T[];
  cursor: string | null;
  constructor(items: T[], cursor: string | null) {
    this.items = items;
    this.cursor = cursor;
  }
  get size(): number {
    return this.items.length;
  }
  first(): T | undefined {
    return this.items[0];
  }
  map<U>(f: (item: T) => U): Page<U> {
    const out: U[] = [];
    for (let i = 0; i < this.items.length; i++) out.push(f(this.items[i]!));
    return new Page<U>(out, this.cursor);
  }
  static empty(): string {
    return 'empty';
  }
}
class CursorPage<T> extends Page<T> {
  hasMore: boolean = false;
  constructor(items: T[], cursor: string | null) {
    super(items, cursor);
    this.hasMore = cursor !== null;
  }
  next(): string {
    return this.cursor ?? 'done';
  }
}
class Box<T = string> {
  value: T;
  constructor(value: T) {
    this.value = value;
  }
  describe(): string {
    return `box(${String(this.value)})`;
  }
}
class Node<T> {
  value: T;
  next: Node<T> | null = null;
  constructor(value: T) {
    this.value = value;
  }
}
class Numbers extends Page<number> {
  total(): number {
    let sum = 0;
    for (let i = 0; i < this.items.length; i++) sum += this.items[i]!;
    return sum;
  }
}
class Shape {
  area(): number {
    return 0;
  }
}
class Square extends Shape {
  side: number;
  constructor(side: number) {
    super();
    this.side = side;
  }
  area(): number {
    return this.side * this.side;
  }
}
class Holder<T extends Shape> {
  shape: T;
  constructor(shape: T) {
    this.shape = shape;
  }
  area(): number {
    return this.shape.area();
  }
}
interface Envelope {
  page: Page<string>;
  tag: string;
}
export function main(): void {
  const page = new Page([1, 2, 3], 'c1');
  const strings = page.map((n: number): string => `#${n}`);
  console.log(`${page.size} ${page.first() ?? -1} ${strings.first() ?? ''} ${strings.cursor} ${Page.empty()}`);
  const cursor = new CursorPage<string>(['a'], null);
  const more = new CursorPage(['x', 'y'], 'c2');
  console.log(`${cursor.hasMore} ${cursor.next()} ${more.hasMore} ${more.next()} ${more.size}`);
  const box: Box = new Box('s');
  const numberBox = new Box<number>(4);
  const contextual: Box<boolean> = new Box(true);
  console.log(`${box.describe()} ${numberBox.describe()} ${contextual.describe()}`);
  const head = new Node(1);
  head.next = new Node(2);
  console.log(`${head.value} ${head.next?.value ?? 0} ${head.next?.next === null}`);
  const numbers = new Numbers([4, 5], null);
  console.log(`${numbers.total()} ${numbers.size} ${numbers.first() ?? 0}`);
  const holder = new Holder<Square>(new Square(3));
  const base = new Holder<Shape>(new Square(2));
  console.log(`${holder.area()} ${base.area()} ${holder.shape.side}`);
  const envelope: Envelope = { page: strings, tag: 't' };
  console.log(JSON.stringify(envelope));
  console.log(JSON.stringify(new CursorPage([true], 'z')));
  const either: Page<number> | string = page;
  const cell: unknown = more;
  const maybe: CursorPage<string> | number = more;
  console.log(`${either instanceof Page} ${maybe instanceof CursorPage ? 'cursor' : 'other'} ${typeof either}`);
  console.log(`${cell instanceof Shape} ${JSON.stringify(cell)}`);
  if (either instanceof Page) console.log(`${either.size}`);
}
