interface P {
  a: number;
  c: number;
}
interface Q {
  a: number;
  b: string;
  c: number;
}
function rest(p: P): string {
  const { a, ...others } = p;
  return String(a) + " " + JSON.stringify(others);
}
function spreadUndefined(p: P | undefined): string {
  return JSON.stringify({ ...p });
}
export function main(): void {
  const q: Q = { c: 3, b: "y", a: 1 };
  console.log(rest(q));
  console.log(rest({ a: 2, c: 4 }));
  console.log(spreadUndefined(q));
  console.log(spreadUndefined(undefined));
}
