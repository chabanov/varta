interface ResponseBody {
  text(): Promise<string>;
  close?(): void;
}
interface Sized {
  size(): number;
}
class Page<T> implements Sized {
  items: T[];
  constructor(items: T[]) {
    this.items = items;
  }
  size(): number {
    return this.items.length;
  }
  map<U>(f: (item: T) => U): Page<U> {
    const out: U[] = [];
    for (let i = 0; i < this.items.length; i++) out.push(f(this.items[i]!));
    return new Page(out);
  }
}
class Transport {
  #calls = 0;
  readonly #fetch: (path: string) => Promise<ResponseBody>;
  constructor(fetch: (path: string) => Promise<ResponseBody>) {
    this.#fetch = fetch;
  }
  async request<T>(path: string, decode: (text: string) => T): Promise<T> {
    this.#calls += 1;
    const body = await this.#fetch(path);
    const text = await body.text();
    body.close?.();
    return decode(text);
  }
  get calls(): number {
    return this.#calls;
  }
}
class Users {
  readonly #client: Transport;
  constructor(client: Transport) {
    this.#client = client;
  }
  // `T` is inferred from the declared result `Promise<Page<string>>`.
  list(): Promise<Page<string>> {
    return this.#client.request('/users', (text: string): Page<string> => new Page([text, `${text}2`]));
  }
}
export async function main(): Promise<void> {
  const transport = new Transport(async (path: string): Promise<ResponseBody> => ({
    async text(): Promise<string> {
      return `user${path.length}`;
    },
  }));
  const page = await new Users(transport).list();
  const lengths = page.map((name: string): number => name.length);
  console.log(`${page.size()} ${lengths.items[1]} ${transport.calls}`);
}
