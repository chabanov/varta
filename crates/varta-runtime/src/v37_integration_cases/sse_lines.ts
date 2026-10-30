// The SDK's server-sent-events loop across the v37 streams: `fetch` of a
// `URL` object (net request), an async generator splitting the streamed
// body with a RegExp (RegExp, async generators), a `Map` of fields
// (collections), a module `let` counter mutated across `await` (language
// core) and a class with `#private` state and a generic method (class
// generics).
const BASE = '__BASE__';

let received = 0;

class Events {
  #fields = new Map<string, string>();
  #count = 0;
  add(line: string): void {
    const match = line.match(/^(\w+): ?(.*)$/);
    if (match !== null) {
      this.#fields.set(match[1] ?? '', match[2] ?? '');
      this.#count += 1;
    }
  }
  read<T>(name: string, parse: (text: string) => T): T | undefined {
    const text = this.#fields.get(name);
    return text === undefined ? undefined : parse(text);
  }
  get count(): number {
    return this.#count;
  }
}

async function* lines(body: ReadableStream): AsyncGenerator<string> {
  let buffer = '';
  for await (const chunk of body) {
    buffer += chunk;
    const parts = buffer.split(/\r?\n/);
    buffer = parts.pop() ?? '';
    for (const line of parts) {
      yield line;
    }
  }
  if (buffer.length > 0) {
    yield buffer;
  }
}

export async function main(): Promise<void> {
  const response = await fetch(new URL('/sse', BASE));
  const body = response.body;
  if (body === null) {
    return;
  }
  const events = new Events();
  for await (const line of lines(body)) {
    if (line === '') {
      received += 1;
      const id = events.read('id', (text: string): number => Number(text)) ?? 0;
      console.log(`${received} ${id} ${events.read('data', (text: string): string => text.toUpperCase())}`);
      continue;
    }
    events.add(line);
  }
  await Promise.resolve();
  console.log(`${received} ${events.count} ${response.url === BASE + '/sse'}`);
}
