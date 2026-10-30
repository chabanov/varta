// Optional and defaulted function-typed parameters, function members of
// unions and `unknown`, narrowing, and `Math.random` as a value (v36).
class Task {
  label: string;
  onDone?: (label: string) => void;
  constructor(label: string) {
    this.label = label;
  }
  finish(): void {
    this.onDone?.(this.label);
  }
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
function notify(message: string, cb?: (m: string) => void): string {
  if (cb) {
    cb(message);
    return 'called';
  }
  return 'skipped';
}
function pick(fn: (() => string) | undefined): string {
  return fn !== undefined ? fn() : 'none';
}
function run(x: (() => string) | string): string {
  if (typeof x === 'function') {
    return x();
  }
  return x;
}
function count(x: (() => number) | number): number {
  if (typeof x !== 'function') {
    return x + 1;
  }
  return x();
}
export function main(): void {
  const t = new Task('build');
  t.finish();
  t.onDone = (label: string): void => {
    console.log(`done ${label}`);
  };
  t.finish();
  t.onDone = undefined;
  t.finish();
  console.log(streamBackoffDelay(3, 100, 1000, (): number => 0.5));
  const delay = streamBackoffDelay(1, 100, 1000);
  console.log(String(delay >= 50 && delay <= 100));
  console.log(`${notify('a')} ${notify('b', (m: string): void => console.log(`got ${m}`))}`);
  console.log(`${pick(undefined)} ${pick((): string => 'some')}`);
  console.log(`${run('plain')} ${run((): string => 'called')} ${count(1)} ${count((): number => 7)}`);
  const v: unknown = (x: number): number => x;
  const u: unknown = 5;
  console.log(`${typeof v} ${typeof u === 'function' ? 'fn' : 'not fn'}`);
  const rnd: () => number = Math.random;
  const x = rnd();
  console.log(`${String(Math.random === rnd)} ${String(x >= 0 && x < 1)}`);
  const maybe: (() => number) | null = null;
  console.log(String(maybe?.()));
}
