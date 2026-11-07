async function v(n: number): Promise<number> { return n; }
function add(a: number, b: number): number { return a + b; }
export async function main(): Promise<void> {
  console.log(String(add(await v(1), await v(2))));
  const x = (await v(3)) * 2 + (await v(4));
  console.log(String(x));
  console.log(`t ${await v(5)} ${await v(6)}`);
  if ((await v(1)) > 0) { console.log("cond"); }
  const o = { a: await v(7), b: [await v(8), await v(9)] };
  console.log(String(o.a + o.b[0] + o.b[1]));
  const y = (await v(0)) > 0 ? await v(10) : await v(11);
  console.log(String(y));
  let i = 0;
  while ((await v(i)) < 3) { i++; }
  console.log(String(i));
}
