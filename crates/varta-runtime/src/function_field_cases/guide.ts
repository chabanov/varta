// The v36 language-guide example (docs/language-guide.md, function fields).
interface Hooks {
  onRetry?: (attempt: number) => void;      // optional function field
  delay: (attempt: number) => number;
}

function backoff(attempt: number, random: () => number = Math.random): number {
  return Math.round(100 * attempt * (1 + random()));
}

function retry(hooks: Hooks, attempts: number): number {
  let total = 0;
  for (let attempt = 1; attempt <= attempts; attempt++) {
    hooks.onRetry?.(attempt);               // skipped when absent
    total += hooks.delay(attempt);          // called without `this`
  }
  return total;
}

export function main(): void {
  const events: string[] = [];
  const hooks: Hooks = {
    onRetry: (attempt: number): void => {
      events.push(`retry ${attempt}`);
    },
    delay: (attempt: number): number => backoff(attempt, (): number => 0.5),
  };
  console.log(String(retry(hooks, 2)));                 // 450
  console.log(JSON.stringify(events));                  // ["retry 1","retry 2"]
  console.log(JSON.stringify(hooks));                   // {} (functions are skipped)

  const controller = new AbortController();
  controller.signal.addEventListener('abort', (): void => {
    events.push(`aborted ${String(controller.signal.reason)}`);
  }, { once: true });
  controller.abort('shutdown');
  console.log(events[2] ?? '');                         // aborted shutdown

  const { promise, resolve } = Promise.withResolvers<string>();
  promise.then((value: string): void => {
    console.log(value);                                 // ready
  });
  resolve('ready');
}
