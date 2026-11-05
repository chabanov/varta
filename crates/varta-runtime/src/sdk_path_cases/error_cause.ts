// Error causes as uarp-sdk 0.13.2 sets and reads them: an SDK error class
// passes `{ cause }` to `super` (core/transport.ts APIConnectionError), and
// a caller reads `error instanceof Error ? error.cause : undefined`. Native
// families take `(message, options)` too.
class ConnectionError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = 'ConnectionError';
  }
}
function causeOf(error: unknown): unknown {
  return error instanceof Error ? error.cause : undefined;
}
function show(value: unknown): string {
  if (value instanceof Error) return `${value.name}(${value.message})`;
  return String(value);
}
export function main(): void {
  const inner = new RangeError('inner');
  const outer = new Error('outer', { cause: inner });
  console.log(outer.message + ' ' + outer.name + ' ' + String(outer instanceof Error) + ' ' + String(outer instanceof RangeError));
  console.log(show(causeOf(outer)));
  console.log(show(causeOf(inner)));
  console.log(show(causeOf(new TypeError('t', { cause: 'text' }))));
  console.log(show(causeOf(new Error('none', {}))));
  console.log(show(causeOf(new Error(undefined, undefined))));
  console.log(outer.toString());
  console.log(JSON.stringify({ e: outer }));
  try {
    throw new TypeError('thrown', { cause: 42 });
  } catch (error) {
    console.log(show(error) + ' ' + show(causeOf(error)) + ' ' + String(error instanceof TypeError));
  }
  const e2 = new SyntaxError('s', { cause: outer });
  console.log(show(e2.cause));
  const wrapped = new ConnectionError('Connection error', { cause: e2 });
  console.log(show(wrapped) + ' <- ' + show(causeOf(wrapped)) + ' <- ' + show(causeOf(causeOf(wrapped))));
  const plain = new TypeError('plain');
  console.log(String(plain.cause));
}
