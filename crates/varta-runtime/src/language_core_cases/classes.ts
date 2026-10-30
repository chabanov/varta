// Class fields: undefined-capable fields without initializers, fields typed by
// `new C()`, and constructor closures capturing `this.field`.
class Counter {
  value = 0;
}
class Stream {
  readonly counter = new Counter();
  readonly controller = new AbortController();
  last: string | undefined;
  closed: boolean | undefined;
  readonly onTick: () => number;
  constructor(signal: AbortSignal | undefined) {
    this.onTick = () => {
      this.counter.value += 1;
      return this.counter.value;
    };
    if (signal) {
      const abort = () => this.controller.abort("outer");
      if (signal.aborted) abort();
      else signal.addEventListener("abort", abort);
    }
  }
}
export function main(): void {
  const outer = new AbortController();
  const stream = new Stream(outer.signal);
  stream.onTick();
  console.log(String(stream.onTick()) + " " + String(stream.last) + " " + String(stream.closed));
  outer.abort();
  console.log(String(stream.controller.signal.aborted) + " " + String(stream.controller.signal.reason));
  const early = new AbortController();
  early.abort();
  console.log(String(new Stream(early.signal).controller.signal.aborted));
}
