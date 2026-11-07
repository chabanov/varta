interface Narrow { a: number; b: number }
interface Wide { b: number; a: number; c: string }
interface Two { a: number; b: number; x: number }
function f(n: Narrow): Two { return { ...n, x: 1 }; }
export function main(): void {
  const w: Wide = { b: 1, a: 2, c: "x" };
  const r = f(w);
  console.log(JSON.stringify(r));
  console.log(JSON.stringify({ ...r }));
}
