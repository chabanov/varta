// Function values in record, interface and class fields, arrays, dictionaries
// and unions (v36 `function_fields`, value layout 17).
interface Handler {
  name: string;
  run: (x: number) => number;
  onDone?: () => void;
}
class Counter {
  count: number = 0;
  step: (n: number) => number;
  constructor(step: (n: number) => number) {
    this.step = step;
  }
  tick(): number {
    this.count = this.step(this.count);
    return this.count;
  }
}
class Relay {
  handler: (n: number) => void = (n: number): void => {
    console.log(`default ${n}`);
  };
  fire(n: number): void {
    this.handler(n);
  }
}
function apply(h: Handler, x: number): number {
  const r = h.run(x);
  h.onDone?.();
  return r;
}
export function main(): void {
  let calls = 0;
  const h: Handler = { name: 'double', run: (x: number): number => x * 2 };
  const g: Handler = {
    name: 'inc',
    run: (x: number): number => x + 1,
    onDone: (): void => {
      calls++;
    },
  };
  console.log(`${apply(h, 21)} ${apply(g, 1)} ${calls}`);
  const c = new Counter((n: number): number => n + 5);
  c.tick();
  console.log(c.tick());
  const relay = new Relay();
  relay.fire(1);
  relay.handler = (n: number): void => {
    console.log(`custom ${n}`);
  };
  relay.fire(2);
  const fs: ((x: number) => number)[] = [h.run, g.run, (x: number): number => x * x];
  fs.push((x: number): number => -x);
  let total = 0;
  for (const f of fs) total += f?.(3) ?? 0;
  const first = fs[0];
  console.log(`${total} ${fs.length} ${first ? first(4) : 0}`);
  console.log(`${typeof h.run} ${String(h.run === fs[0])} ${String(h.run === g.run)}`);
  console.log(`${String(fs.includes(g.run))} ${fs.indexOf(g.run)}`);
  const table: Record<string, (x: number) => number> = { a: h.run, b: g.run };
  table['c'] = (x: number): number => x - 1;
  console.log((table['a']?.(10) ?? 0) + (table.b?.(10) ?? 0) + (table['c']?.(10) ?? 0));
  console.log(JSON.stringify(Object.keys(table)));
  const { run } = g;
  console.log(run(9));
  console.log(JSON.stringify(h));
  console.log(JSON.stringify(g));
  console.log(JSON.stringify(fs));
  console.log(JSON.stringify(table));
}
