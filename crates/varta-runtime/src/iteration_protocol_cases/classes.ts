// Iterable classes: `[Symbol.iterator]()` generator methods, inherited and
// overridden, on generic classes, through `implements Iterable<T>`, the
// `Iterator<T>` and `IterableIterator<T>` annotations and explicit calls.
class Bag implements Iterable<number> {
  items: number[] = [1, 2, 3];
  *[Symbol.iterator](): Generator<number> {
    for (const x of this.items) yield x;
  }
}
class Range implements Iterable<number> {
  constructor(private start: number, private end: number) {}
  *[Symbol.iterator](): IterableIterator<number> {
    for (let i = this.start; i < this.end; i++) yield i;
  }
}
class List<T> {
  items: T[] = [];
  add(x: T): List<T> {
    this.items.push(x);
    return this;
  }
  *[Symbol.iterator](): Iterator<T> {
    for (const x of this.items) yield x;
  }
  *indexed(from: number): Generator<string, number> {
    let n = from;
    for (const x of this.items) yield `${n++}:${x}`;
    return n;
  }
}
class Countdown {
  constructor(public from: number) {}
  [Symbol.iterator](): Generator<number> {
    return countdown(this.from);
  }
}
function* countdown(from: number): Generator<number> {
  for (let i = from; i > 0; i--) yield i;
}
class Shelf extends Bag {
  *[Symbol.iterator](): Generator<number> {
    for (const x of this.items) yield x * 10;
  }
}
function total(bag: Bag): number {
  let sum = 0;
  for (const x of bag) sum += x;
  return sum;
}
export function main(): void {
  const b = new Bag();
  for (const x of b) console.log(`bag ${x}`);
  console.log(`${total(b)} ${total(new Shelf())}`);
  b.items.push(4);
  console.log(`live ${total(b)}`);
  console.log([...new Range(10, 13)].join(','));
  const l = new List<string>().add('p').add('q');
  for (const s of l) console.log(s);
  const numbers = new List<number>().add(5);
  console.log(`${[...numbers, ...l].join('|')}`);
  const lines = l.indexed(7);
  let step = lines.next();
  while (!step.done) {
    console.log(step.value);
    step = lines.next();
  }
  console.log(`next index ${step.value}`);
  console.log([...new Countdown(3)].join(' '));
  const it = b[Symbol.iterator]();
  const first = it.next();
  const second = it.next();
  console.log(`${first.value} ${second.value} ${first.done}`);
  const manual: Iterator<number> = new Range(0, 2)[Symbol.iterator]();
  console.log(`${manual.next().value} ${manual.next().value} ${manual.next().done}`);
  const copies: number[][] = [];
  for (const x of new Range(0, 3)) {
    copies.push([...new Range(0, x)]);
  }
  console.log(copies.map((c) => c.join('')).join('/'));
}
