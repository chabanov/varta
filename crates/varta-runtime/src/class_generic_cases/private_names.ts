type Fetcher = (url: string) => Promise<string>;
class Counter {
  #count = 0;
  readonly #step: number;
  #log: string[] = [];
  readonly #fetch: Fetcher;
  constructor(step: number, fetch: Fetcher) {
    this.#step = step;
    this.#fetch = fetch;
  }
  get value(): number {
    return this.#count;
  }
  bump(): number {
    this.#count += this.#step;
    this.#record(`bump ${this.#count}`);
    return this.#count;
  }
  #record(line: string): void {
    this.#log.push(line);
  }
  get #size(): number {
    return this.#log.length;
  }
  async #send(path: string): Promise<string> {
    return await this.#fetch(path);
  }
  async load(path: string): Promise<string> {
    const text = await this.#send(path);
    this.#record(text);
    return `${text} ${this.#size}`;
  }
  same(other: Counter): boolean {
    return other.#step === this.#step;
  }
  static is(value: Counter | number[]): boolean {
    return #count in value;
  }
  history(): string {
    let out = '';
    for (let i = 0; i < this.#log.length; i++) out += `${this.#log[i]};`;
    return out;
  }
}
class Sub extends Counter {
  count: number = 100;
  more(): number {
    return this.bump() + this.count;
  }
}
export async function main(): Promise<void> {
  const c = new Counter(2, async (url: string): Promise<string> => `got ${url}`);
  c.bump();
  c.bump();
  console.log(`${c.value} ${await c.load('/a')}`);
  const s = new Sub(5, async (url: string): Promise<string> => url);
  console.log(`${s.more()} ${s.count} ${s.value} ${c.same(s)} ${c.same(new Counter(2, async (u: string): Promise<string> => u))}`);
  console.log(`${Counter.is(c)} ${Counter.is(s)} ${Counter.is([1])}`);
  console.log(c.history());
}
