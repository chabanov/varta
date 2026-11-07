interface P {
  a: number;
}
interface R {
  r: number;
}
interface Q {
  a: number;
  b: string;
}
interface S {
  r: number;
  s: boolean;
  b: string;
}
function merge(p: P, r: R | undefined): string {
  return JSON.stringify({ first: 1, ...p, mid: 2, ...r, last: 3 });
}
export function main(): void {
  const q: Q = { b: "y", a: 1 };
  const s: S = { s: true, r: 5, b: "later" };
  console.log(merge(q, s));
  console.log(merge(q, undefined));
  console.log(merge({ a: 7 }, s));
  console.log(merge({ a: 8 }, { r: 9 }));
}
