// Function values stored in interface and class fields, arrays and a
// dictionary, an optional callback field and a defaulted function-typed
// parameter; each is called through its container.
interface Hooks {
  onRetry?: (attempt: number) => void;
  delay: (attempt: number) => number;
}
class Step {
  apply: (n: number) => number;
  constructor(apply: (n: number) => number) {
    this.apply = apply;
  }
}
function backoff(attempt: number, random: () => number = Math.random): number {
  return Math.round(100 * attempt * (1 + random()));
}
function retry(hooks: Hooks, attempts: number): number {
  let total = 0;
  for (let attempt = 1; attempt <= attempts; attempt++) {
    hooks.onRetry?.(attempt);
    total += hooks.delay(attempt);
  }
  return total;
}
export function main(): void {
  let seen = '';
  const hooks: Hooks = {
    onRetry: (attempt: number): void => {
      seen = seen + String(attempt);
    },
    delay: (attempt: number): number => backoff(attempt, (): number => 0.5),
  };
  console.log(`${retry(hooks, 3)} ${seen} ${retry({ delay: (n: number): number => n }, 2)}`);
  const steps: Step[] = [new Step((n: number): number => n + 1), new Step((n: number): number => n * 3)];
  let value = 1;
  for (const step of steps) value = step?.apply(value) ?? value;
  const table: Record<string, (n: number) => number> = { half: (n: number): number => n / 2 };
  console.log(`${value} ${table['half']?.(value) ?? 0} ${typeof hooks.delay} ${JSON.stringify(hooks)}`);
}
