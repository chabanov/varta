// The pilot's cancellation pattern without timers: a signal checked before and
// between awaited steps, its reason rethrown, caught as `unknown` and narrowed
// (v35 `abort_signals`).
class Client {
  private readonly controller: AbortController;
  constructor() {
    this.controller = new AbortController();
  }
  get signal(): AbortSignal {
    return this.controller.signal;
  }
  cancel(reason?: unknown): void {
    this.controller.abort(reason);
  }
}

async function step(n: number, signal: AbortSignal | undefined): Promise<number> {
  if (signal !== undefined) signal.throwIfAborted();
  await null;
  return n * 2;
}

async function run(client: Client, cancelAt: number): Promise<string> {
  let total = 0;
  try {
    for (let n = 1; n <= 4; n++) {
      if (n === cancelAt) client.cancel();
      total += await step(n, client.signal);
    }
    return `done ${total}`;
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') {
      return `aborted after ${total}: ${error.message}`;
    }
    return 'unexpected';
  }
}

export async function main(): Promise<void> {
  console.log(await run(new Client(), 0));
  console.log(await run(new Client(), 3));
  const custom = new Client();
  custom.cancel(new RangeError('limit'));
  try {
    await step(1, custom.signal);
  } catch (error) {
    if (error instanceof RangeError) console.log(`custom ${error.message}`);
  }
  const signals: AbortSignal[] = [AbortSignal.abort('x'), new AbortController().signal];
  let aborted = 0;
  for (const signal of signals) {
    if (signal !== undefined && signal.aborted) aborted++;
  }
  console.log(`aborted ${aborted} of ${signals.length}`);
}
