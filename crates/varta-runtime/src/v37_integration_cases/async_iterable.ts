// `implements AsyncIterable<T>` (class generics) on a class whose
// `[Symbol.asyncIterator]()` is an async generator method (net request), as
// the SDK's event stream declares it.
class Ticks implements AsyncIterable<number> {
  #limit: number;
  constructor(limit: number) {
    this.#limit = limit;
  }
  async *[Symbol.asyncIterator](): AsyncGenerator<number> {
    for (let i = 1; i <= this.#limit; i++) {
      await Promise.resolve();
      yield i * 10;
    }
  }
}
export async function main(): Promise<void> {
  let total = 0;
  for await (const tick of new Ticks(3)) {
    total += tick;
  }
  console.log(String(total));
}
