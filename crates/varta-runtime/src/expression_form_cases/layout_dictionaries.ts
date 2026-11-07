interface Narrow {
  a: number;
  b: number;
}
interface Wide {
  b: number;
  a: number;
  c: string;
}
function entries(n: Narrow): string {
  const q: Record<string, unknown> = { first: 0, ...n };
  return Object.keys(q).join(",") + " " + JSON.stringify(q);
}
function held(value: unknown): string {
  const record = value as Record<string, unknown>;
  return Object.keys(record).join(",");
}
export function main(): void {
  const w: Wide = { b: 1, a: 2, c: "x" };
  console.log(entries(w));
  console.log(entries({ a: 3, b: 4 }));
  const n: Narrow = w;
  console.log(held(n));
}
