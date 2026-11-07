interface Counter { count: number; inc(): Counter }
class Range implements Iterable<number> {
  last: number;
  constructor(last: number) { this.last = last; }
  [Symbol.iterator](): Iterator<number> {
    let n = 0;
    const last = this.last;
    return { next(): IteratorResult<number> { return n < last ? { value: n++, done: false } : { value: undefined, done: true }; } };
  }
}
class Lock implements Disposable {
  [Symbol.dispose](): void { console.log("released"); }
}
async function value(n: number): Promise<number> { return n; }
function max3(a: number, b: number, c: number): number { return Math.max(a, b, c); }
export async function main(): Promise<void> {
  const counter: Counter = { count: 0, inc() { this.count++; return this; } };
  console.log(String(counter.inc().inc().count)); // 2
  const key = "total";
  const sums = { [key]: (await value(1)) + (await value(2)) };
  console.log(String(sums.total)); // 3
  const [a, b, ...more] = new Range(5);
  console.log(`${a} ${b} ${more.join(",")}`); // 0 1 2,3,4
  const args: [number, number, number] = [3, 9, 4];
  console.log(String(max3(...args))); // 9
  console.log(String.raw`\d+${a}`); // \d+0
  {
    using lock = new Lock();
    console.log("locked");
  } // locked, then released
}
