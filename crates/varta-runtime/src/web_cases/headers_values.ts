// Headers and AbortSignal as ordinary nominal values: class fields, the
// pilot's numericHeader, unions, `unknown`, closures, generator and async
// frames, and subclasses (v35 `web_headers`, `abort_signals`).
class Request {
  readonly headers: Headers;
  readonly signal: AbortSignal | null;
  constructor(init: HeadersInit, signal: AbortSignal | null) {
    this.headers = new Headers(init);
    this.signal = signal;
  }
}

class TracedHeaders extends Headers {
  entries(): string[][] {
    return [];
  }
}

function numericHeader(headers: Headers, name: string): number | undefined {
  const value = headers.get(name);
  if (value === null) return undefined;
  const parsed = Number(value);
  return Number.isNaN(parsed) ? undefined : parsed;
}

function describe(value: unknown): string {
  if (value instanceof Headers) return `headers ${value.get('x-id')}`;
  if (value instanceof AbortSignal) return `signal ${value.aborted}`;
  if (value instanceof AbortController) return `controller ${value.signal.aborted}`;
  return typeof value;
}

function check(signal?: AbortSignal): string {
  if (signal !== undefined && signal.aborted) return 'aborted';
  return 'live';
}

function* reads(headers: Headers): Generator<string> {
  for (const [name, value] of headers) {
    yield `${name}:${value}`;
  }
}

async function later(headers: Headers, controller: AbortController): Promise<string> {
  await null;
  controller.abort();
  return `${headers.get('retry-after')} ${controller.signal.aborted}`;
}

function makeCounter(headers: Headers): (name: string) => string {
  let calls = 0;
  return (name: string): string => {
    calls++;
    headers.append('x-call', String(calls));
    return `${headers.get(name)}`;
  };
}

export async function main(): Promise<void> {
  const controller = new AbortController();
  const request = new Request({ 'Retry-After': '30', 'X-Id': 'a1' }, controller.signal);
  console.log(String(numericHeader(request.headers, 'retry-after')));
  console.log(String(numericHeader(request.headers, 'x-id')));
  console.log(String(numericHeader(request.headers, 'missing')));
  const values: unknown[] = [request.headers, controller.signal, controller, 'text'];
  for (const value of values) console.log(describe(value));
  const either: Headers | string = values.length > 1 ? request.headers : 'none';
  if (either instanceof Headers) console.log(`union ${either.has('x-id')}`);
  console.log(`${check()} ${check(controller.signal)}`);
  const counter = makeCounter(request.headers);
  console.log(counter('x-call'));
  console.log(counter('x-call'));
  for (const line of reads(request.headers)) console.log(line);
  const result = await later(request.headers, controller);
  console.log(result);
  console.log(check(request.signal ?? undefined));
  // Iteration uses Headers.prototype.entries, like [Symbol.iterator].
  const traced = new TracedHeaders([['b', '2'], ['a', '1']]);
  let names = '';
  for (const [name] of traced) names = names + name;
  console.log(`${names} ${traced.entries().length} ${traced instanceof Headers}`);
}
