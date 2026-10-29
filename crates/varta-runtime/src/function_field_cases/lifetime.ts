// Closure environments reachable only through fields, elements, dictionary
// entries, union and `unknown` cells survive collections: the program keeps
// them across awaits while the frames are queued, and the driver forces a
// collection before every job.
interface Counter {
  label: string;
  next: () => number;
  peek?: () => string;
}
class Holder {
  read: () => string;
  constructor(read: () => string) {
    this.read = read;
  }
}
function counter(label: string, start: number): Counter {
  let value = start;
  const suffix = `${label}!`;
  return {
    label,
    next: (): number => {
      value += 1;
      return value;
    },
    peek: (): string => `${suffix}${value}`,
  };
}
function holder(text: string): Holder {
  const copy = `${text}/${text.length}`;
  return new Holder((): string => copy);
}
async function step(n: number): Promise<number> {
  return n;
}
async function run(): Promise<string> {
  const counters: Counter[] = [counter('a', 1), counter('b', 10)];
  const table: Record<string, () => string> = {};
  table['k'] = holder('key').read;
  const cell: unknown = holder('cell');
  const either: (() => number) | string = counters[1]?.next ?? 'none';
  await step(1);
  let total = 0;
  for (const c of counters) {
    total += c?.next() ?? 0;
  }
  await step(2);
  const fromCell = cell instanceof Holder ? cell.read() : 'lost';
  const fromUnion = typeof either === 'function' ? either() : -1;
  await step(3);
  return `${total} ${counters[0]?.peek?.() ?? ''} ${table['k']?.() ?? ''} ${fromCell} ${fromUnion}`;
}
export function main(): void {
  run().then((text: string): void => {
    console.log(text);
  });
}
