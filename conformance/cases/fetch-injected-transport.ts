// The injected-transport shape of a typed HTTP SDK: a `FetchLike` that
// answers with constructed Responses, a retry loop over transient statuses,
// Request/Headers construction and JSON/text bodies. No network is used.
type FetchLike = (input: string, init: RequestInit) => Promise<Response>;

interface Item {
  id: string;
  count: number;
}

const RETRYABLE = new Set<number>([429, 503]);

class Client {
  readonly #fetch: FetchLike;
  readonly #base: string;
  attempts = 0;
  constructor(base: string, fetchImpl: FetchLike) {
    this.#base = base.replace(/\/+$/, '');
    this.#fetch = fetchImpl;
  }
  async get(path: string): Promise<Item> {
    for (;;) {
      this.attempts++;
      const response = await this.#fetch(`${this.#base}${path}`, {
        method: 'GET',
        headers: { Accept: 'application/json', 'X-Attempt': String(this.attempts) },
      });
      if (response.ok) return (await response.json()) as Item;
      if (!RETRYABLE.has(response.status) || this.attempts >= 3) {
        throw new Error(`HTTP ${response.status}: ${await response.text()}`);
      }
    }
  }
}

export async function main(): Promise<void> {
  const seen: string[] = [];
  const statuses = [503, 429, 200];
  let calls = 0;
  const transport: FetchLike = async (input, init) => {
    const request = new Request(input, init);
    seen.push(`${request.method} ${request.url} ${request.headers.get('x-attempt')} ${request.headers.get('accept')}`);
    const status = statuses[calls++] ?? 500;
    if (status !== 200) return new Response('busy', { status, statusText: 'Busy' });
    return new Response(JSON.stringify({ id: 'a1', count: 3 }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  };
  const client = new Client('https://api.example.test/v1//', transport);
  const item = await client.get('/items/a1');
  console.log(`${item.id} ${item.count} ${client.attempts}`);
  for (const line of seen) {
    console.log(line);
  }
  const failing = new Client('https://api.example.test', async () => new Response('missing', { status: 404 }));
  try {
    await failing.get('/x');
  } catch (error) {
    console.log(error instanceof Error ? error.message : 'other');
  }
  const headers = new Headers({ 'X-B': '2', 'x-a': '1' });
  headers.append('X-A', '3');
  const names: string[] = [];
  headers.forEach((value, name) => {
    names.push(`${name}=${value}`);
  });
  console.log(names.join(';'));
  const created = new Response(null, { status: 204 });
  const empty = await created.text();
  console.log(`${created.status} ${created.ok} ${empty.length}`);
}
