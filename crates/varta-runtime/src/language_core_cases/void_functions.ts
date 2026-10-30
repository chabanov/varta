// Functions returning values are assignable to void-returning function types:
// arguments, annotated bindings, record fields (also optional ones) and
// annotated arrows in a void context.
interface Box { n: number }
function run(cb: () => void): void {
  cb();
  cb();
}
function each(xs: number[], cb: (x: number) => void): void {
  for (const x of xs) cb(x);
}
interface Hooks { onDone: () => void; onState?: (state: number) => void }
export function main(): void {
  const box: Box = { n: 0 };
  const bump = (): number => {
    box.n += 1;
    return box.n;
  };
  run(bump);
  const later: () => void = bump;
  later();
  const pushed: number[] = [];
  each([1, 2, 3], (x: number): number => pushed.push(x * 10));
  const hooks: Hooks = { onDone: bump, onState: (state: number): number => pushed.push(state) };
  hooks.onDone();
  hooks.onState?.(7);
  const asUndefined = (): undefined => undefined;
  run(asUndefined);
  console.log(String(box.n) + " " + String(pushed.length) + " " + String(pushed[3]));
}
