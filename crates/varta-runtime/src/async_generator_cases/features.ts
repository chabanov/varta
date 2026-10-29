// Generic async generators, methods, closures over body locals, switch and
// labelled loops in the body, optional and default parameters, re-entrant
// requests from inside the running body.
function say(text: string): void {
  console.log(text);
}
interface Page<T> {
  items: T[];
  more: boolean;
}
async function* pages<T>(all: Page<T>[]): AsyncIterableIterator<T> {
  for (const page of all) {
    if (page === undefined) continue;
    const fetched = await Promise.resolve(page);
    for (const item of fetched.items) if (item !== undefined) yield item;
    if (!fetched.more) return;
  }
}
async function collect<T>(iterator: AsyncIterable<T>, limit: number): Promise<T[]> {
  const out: T[] = [];
  for await (const item of iterator) {
    out.push(item);
    if (out.length >= limit) break;
  }
  return out;
}
class Lines {
  prefix: string;
  constructor(prefix: string) {
    this.prefix = prefix;
  }
  async *split(text: string, limit?: number): AsyncGenerator<string, void, undefined> {
    let buffer = "";
    let count = 0;
    const flush = (): string | undefined => {
      if (buffer.length === 0) return undefined;
      const line = this.prefix + buffer;
      buffer = "";
      count++;
      return line;
    };
    for (let i = 0; i < text.length; i++) {
      const c = text.slice(i, i + 1);
      switch (c === "\n" ? 1 : 0) {
        case 1: {
          const line = flush();
          if (line) yield line;
          if (limit !== undefined && count >= limit) return;
          break;
        }
        default:
          buffer += c;
      }
    }
    const last = flush();
    if (last) yield last;
  }
  static async *range(from: number, to: number = from + 3): AsyncGenerator<number, void, undefined> {
    outer: for (let i = from; i < to; i++) {
      for (let j = 0; j < 3; j++) {
        if (j === 1) continue outer;
        yield i * 10 + j;
      }
    }
  }
}
async function run(): Promise<void> {
  const numbers = await collect(pages<number>([{ items: [1, 2], more: true }, { items: [3], more: true }, { items: [4, 5], more: false }, { items: [6], more: false }]), 10);
  say("numbers " + JSON.stringify(numbers));
  const names = await collect(pages<string>([{ items: ["a", "b", "c"], more: true }]), 2);
  say("names " + JSON.stringify(names));
  const lines = new Lines("> ");
  for await (const line of lines.split("one\ntwo\n\nthree")) say(line);
  for await (const line of lines.split("a\nb\nc\nd", 2)) say(line);
  for await (const n of Lines.range(1)) say("range " + String(n));
}
async function* echo(): AsyncGenerator<number, string, number> {
  let total = 0;
  for (;;) {
    const n = yield total;
    if (n < 0) return "stop " + String(total);
    total += n;
    if (total > 100) break;
  }
  return "overflow";
}
function reentrant(): void {
  const it = echo();
  it.next(0).then((r) => say("e0 " + JSON.stringify(r)));
  it.next(5).then((r) => say("e1 " + JSON.stringify(r)));
  it.next(7).then((r) => say("e2 " + JSON.stringify(r)));
  it.next(-1).then((r) => say("e3 " + JSON.stringify(r)));
  it.next(9).then((r) => say("e4 " + JSON.stringify(r)));
}
export function main(): void {
  run();
  reentrant();
}
