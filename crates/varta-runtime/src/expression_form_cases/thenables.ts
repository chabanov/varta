interface Th { then(ok: (v: number) => void, fail: (e: unknown) => void): void }
class Lazy {
  n: number;
  constructor(n: number) { this.n = n; }
  then(ok: (v: string) => void): void { console.log("then called " + String(this.n)); ok("lazy" + String(this.n)); }
}
async function tick(label: string): Promise<void> {
  await null;
  console.log(label + " 1");
  await null;
  console.log(label + " 2");
  await null;
  console.log(label + " 3");
}
export async function main(): Promise<void> {
  const ok: Th = { then(resolve: (v: number) => void, fail: (e: unknown) => void): void { console.log("ok then"); resolve(7); } };
  const bad: Th = { then(resolve: (v: number) => void, fail: (e: unknown) => void): void { fail(new Error("refused")); } };
  const ticks = tick("t");
  console.log("before");
  const v = await ok;
  console.log("got " + String(v));
  try {
    await bad;
  } catch (e) {
    console.log("caught " + (e as Error).message);
  }
  const s = await new Lazy(3);
  console.log(s);
  const p = Promise.resolve(new Lazy(4));
  console.log("resolve returned");
  console.log(await p);
  await ticks;
}
