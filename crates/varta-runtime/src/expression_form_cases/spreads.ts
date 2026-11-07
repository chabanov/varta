function sum(a: number, b: number, c: number): number { return a + b + c; }
function all(...xs: number[]): number { return xs.reduce((s, x) => s + x, 0); }
function label(name: string, n: number): string { return name + "=" + String(n); }
class P { x: number; y: number; constructor(x: number, y: number) { this.x = x; this.y = y; } }
export function main(): void {
  const xs: [number, number, number] = [1, 2, 3];
  console.log(String(sum(...xs)));
  console.log(String(sum(...[4, 5], 6)));
  const ys = [4, 5, 6];
  console.log(String(all(...ys, 7, ...ys)));
  console.log(String(Math.max(...ys)) + " " + String(Math.min(...ys, 2)) + " " + String(Math.max(...[])));
  const pair: [number, number] = [8, 9];
  const pt = new P(...pair);
  console.log(String(pt.x + pt.y));
  const entry: [string, number] = ["k", 3];
  console.log(label(...entry));
}
