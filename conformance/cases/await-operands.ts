async function v(n: number): Promise<number> { return n; }
async function s(t: string): Promise<string> { return t; }
export async function main(): Promise<void> {
  const xs = [await v(1), ...[await v(2)]];
  console.log(xs.join(","));
  const m = Math.max(await v(3), await v(4));
  console.log(String(m));
  const b = (await v(1)) === 1 && (await s("y")) === "y";
  console.log(String(b));
  for (const x of [await v(5)]) { console.log(String(x)); }
  console.log((await s("abc")).toUpperCase());
  const n = -(await v(6));
  console.log(String(n));
}
