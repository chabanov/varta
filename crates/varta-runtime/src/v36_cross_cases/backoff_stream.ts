// V36 async generators with timers and function fields: the pilot's SSE
// reconnect shape. An async generator sleeps between reconnect attempts with
// the pilot's `streamBackoffDelay` (an injected `random`), a `for await`
// consumes it while an interval ticks, and `break` closes a generator that is
// suspended at a `yield`.
interface StreamHooks {
  onRetry?: (attempt: number, delay: number) => void;
  random: () => number;
}

function streamBackoffDelay(
  attempt: number,
  baseIntervalMs: number,
  maxDelayMs: number,
  random: () => number = Math.random,
): number {
  const exponential = baseIntervalMs * 2 ** Math.max(attempt - 1, 0);
  const maxSleep = Math.min(maxDelayMs, exponential);
  const half = Math.max(maxSleep / 2, 1);
  return Math.round(half + random() * half);
}

function sleep(ms: number): Promise<void> {
  return new Promise<void>((resolve) => setTimeout(resolve, ms));
}

async function* events(
  chunks: string[],
  hooks: StreamHooks,
): AsyncGenerator<string, number, undefined> {
  let attempt = 0;
  let delivered = 0;
  for (const chunk of chunks) {
    if (chunk === undefined) continue;
    if (chunk === 'drop') {
      attempt++;
      const delay = streamBackoffDelay(attempt, 20, 200, hooks.random);
      hooks.onRetry?.(attempt, delay);
      await sleep(delay);
      continue;
    }
    attempt = 0;
    delivered++;
    yield chunk;
  }
  return delivered;
}

export async function main(): Promise<void> {
  let ticks = 0;
  const interval = setInterval(() => {
    ticks++;
    console.log('tick ' + String(ticks));
  }, 70);
  const hooks: StreamHooks = {
    onRetry: (attempt: number, delay: number): void => {
      console.log('retry ' + String(attempt) + ' in ' + String(delay));
    },
    random: (): number => 0.5,
  };
  for await (const event of events(['a', 'drop', 'drop', 'b', 'drop', 'drop', 'drop', 'c'], hooks)) {
    console.log('event ' + event + ' after ' + String(ticks) + ' ticks');
  }
  clearInterval(interval);
  const manual = events(['m', 'drop', 'n'], hooks);
  let step = await manual.next();
  while (step.done !== true) {
    console.log('manual ' + String(step.value));
    step = await manual.next();
  }
  console.log('returned ' + String(step.value));
  const stream = events(['x', 'drop', 'y', 'z'], { random: Math.random });
  for await (const event of stream) {
    console.log('first ' + event);
    break;
  }
  const after = await stream.next();
  console.log('closed ' + String(after.done) + ' ' + String(after.value));
}
