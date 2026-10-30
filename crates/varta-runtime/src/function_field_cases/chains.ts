// Optional chains (v36): member reads, method and field calls and optional
// calls short circuit on `undefined` and `null` and evaluate their object once.
interface Inner {
  label: string;
  run?: (n: number) => number;
}
interface Outer {
  inner?: Inner;
  name: string;
}
class Box {
  value: number;
  constructor(value: number) {
    this.value = value;
  }
  twice(): number {
    return this.value * 2;
  }
}
function describe(o: Outer | undefined): string {
  const label = o?.inner?.label;
  const name = o?.name;
  return `${String(label)} ${String(name)}`;
}
export function main(): void {
  console.log(describe(undefined));
  console.log(describe({ name: 'n' }));
  console.log(describe({ name: 'm', inner: { label: 'x' } }));
  const o: Outer = { name: 'k', inner: { label: 'y', run: (n: number): number => n + 1 } };
  const p: Outer = { name: 'k', inner: { label: 'z' } };
  console.log(`${String(o.inner?.run?.(41))} ${String(p.inner?.run?.(41))}`);
  const b: Box | undefined = new Box(4);
  const c: Box | null = null;
  console.log(`${String(b?.twice())} ${String(c?.twice())} ${String(b?.value)}`);
  const xs: number[] | undefined = [1, 2, 3];
  const s: string | undefined = 'abc';
  console.log(`${String(xs?.length)} ${String(s?.length)}`);
  let evaluated = 0;
  const side = (): number => {
    evaluated++;
    return evaluated;
  };
  const none: ((n: number) => number) | undefined = undefined;
  const some: ((n: number) => number) | undefined = (n: number): number => n * 10;
  console.log(`${String(none?.(side()))} ${String(some?.(side()))} ${evaluated}`);
  let reads = 0;
  const source = (): Outer | undefined => {
    reads++;
    return o;
  };
  console.log(`${String(source()?.inner?.label)} ${reads}`);
  const signal: AbortSignal | undefined = AbortSignal.abort('stop');
  if (signal?.aborted) {
    console.log(String(signal.reason));
  }
}
