const trace: string[] = [];
async function v(n: number, tag: string): Promise<number> { trace.push(tag); return n; }
async function s(t: string): Promise<string> { trace.push(t); return t; }
function side(tag: string): number { trace.push(tag); return 1; }
class Box {
  n: number;
  constructor(n: number) { this.n = n; }
  add(k: number): number { return this.n + k; }
}
interface Rec { a: number; b?: number; f?: (x: number) => number }
export async function main(): Promise<void> {
  console.log(String(side("a") + (await v(2, "b")) + side("c")));
  const box = new Box(await v(5, "new"));
  console.log(String(box.add(await v(1, "arg"))));
  console.log(String(new Box(10).add((await v(3, "x")) * (await v(4, "y")))));
  const r: Rec = { a: await v(1, "fa"), b: await v(2, "fb") };
  r.a = await v(7, "set");
  r.a += await v(1, "compound");
  console.log(`${r.a} ${r.b}`);
  const xs = [1, 2, 3];
  xs[await v(0, "idx")] = await v(9, "val");
  console.log(xs.join(","));
  const maybe: Rec | undefined = undefined;
  console.log(String(maybe?.f?.(await v(1, "never-or-not"))));
  console.log(String(r.f?.(await v(1, "opt"))));
  console.log(`${await s("t1")}-${(await s("t2")).length}`);
  const joined = [await s("p"), ...(await s("qr")).split("")].join("+");
  console.log(joined);
  let flag = false;
  flag ||= (await v(1, "or")) > 0;
  console.log(String(flag));
  let total = 0;
  for (let i = 0; i < (await v(2, "test")); i++) { total += await v(i, "body"); }
  console.log(String(total));
  const pick = (await v(1, "c")) > 0 ? (await s("yes")) : (await s("no"));
  console.log(pick);
  const mapped = xs.map((x) => x * 2).filter((x) => x > 2);
  console.log(String(mapped.length + (await v(1, "after"))));
  switch (await v(2, "disc")) {
    case 1: console.log("one"); break;
    case 2: console.log("two " + String(await v(3, "case"))); break;
  }
  try {
    const bad = await Promise.reject<number>(new Error("nope"));
    console.log(String(bad));
  } catch (e) {
    console.log("caught " + (e as Error).message + " " + String(await v(1, "catch")));
  }
  console.log(trace.join(" "));
}
