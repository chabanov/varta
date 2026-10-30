// Receivers, private fields and closures of generic async method instances
// stay rooted while their frames are suspended: the driver forces a
// collection before every job.
interface Page<T> {
  items: T[];
  next: string | null;
}
class Client {
  #calls = 0;
  readonly #prefix: string;
  readonly #fetch: (path: string) => Promise<string>;
  constructor(prefix: string, fetch: (path: string) => Promise<string>) {
    this.#prefix = prefix;
    this.#fetch = fetch;
  }
  async request<T>(path: string, decode: (text: string) => T): Promise<T> {
    this.#calls += 1;
    const text = await this.#fetch(`${this.#prefix}${path}`);
    const value = decode(text);
    await this.#tick();
    return value;
  }
  async #tick(): Promise<void> {
    await Promise.resolve();
  }
  get calls(): number {
    return this.#calls;
  }
}
class Lister {
  readonly client: Client;
  constructor(client: Client) {
    this.client = client;
  }
  page(n: number): Promise<Page<string>> {
    return this.client.request(`/p${n}`, (text: string): Page<string> => ({
      items: [`${text}:a`, `${text}:b`],
      next: n < 2 ? `c${n + 1}` : null,
    }));
  }
  count(path: string): Promise<number> {
    return this.client.request(path, (text: string): number => text.length);
  }
}
export async function main(): Promise<void> {
  const seen: string[] = [];
  const lister = new Lister(
    new Client('api', async (path: string): Promise<string> => {
      seen.push(path);
      return `<${path}>`;
    }),
  );
  let collected = '';
  for (let n = 1; n <= 2; n++) {
    const page = await lister.page(n);
    collected += `${page.items[0]}|${page.items[1]}|${page.next ?? 'end'};`;
  }
  const size = await lister.count('/n');
  console.log(`${collected} ${size} ${lister.client.calls} ${seen.length}`);
}
