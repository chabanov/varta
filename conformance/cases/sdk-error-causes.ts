// V38 SDK paths: Error options with a `cause` passed through `super` and to
// native families, causes read through `unknown`, and `instanceof` tests of
// Map, Set and Uint8Array members of closed unions.
class APIConnectionError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = 'APIConnectionError';
  }
}

function chain(error: unknown): string {
  const names: string[] = [];
  let current: unknown = error;
  while (current instanceof Error) {
    names.push(`${current.name}:${current.message}`);
    current = current.cause;
  }
  if (current !== undefined) names.push(`value:${String(current)}`);
  return names.join(' <- ');
}

function bytes(input: Uint8Array | string): string {
  return input instanceof Uint8Array ? `bytes ${input.length}` : `text ${input.length}`;
}
function mapped(input: Map<string, string> | string | undefined): string {
  if (input instanceof Map) return `map ${input.size}`;
  return input ?? 'none';
}
function counted(input: Set<number> | number): string {
  return input instanceof Set ? `set ${input.has(1)}` : `number ${input}`;
}

export function main(): void {
  const root = new TypeError('socket closed', { cause: 'ECONNRESET' });
  const wrapped = new APIConnectionError('Connection error', { cause: root });
  console.log(chain(wrapped));
  console.log(`${wrapped instanceof APIConnectionError} ${wrapped instanceof Error} ${wrapped.toString()}`);
  console.log(chain(new RangeError('plain')));
  console.log(chain(new Error('empty options', {})));
  try {
    throw new APIConnectionError('retry exhausted', { cause: wrapped });
  } catch (error) {
    console.log(chain(error));
  }
  console.log(`${bytes(new Uint8Array([1, 2, 3]))}, ${bytes('text')}`);
  const map = new Map<string, string>([['a', 'b']]);
  console.log(`${mapped(map)}, ${mapped('text')}, ${mapped(undefined)}, ${counted(new Set<number>([1]))}, ${counted(2)}`);
}
