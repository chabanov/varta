interface P {
  a: number;
}
interface Q {
  a: number;
  b: string;
}
function keys(p: P): string {
  const names: string[] = [];
  for (const key in p) {
    names.push(key);
  }
  return Object.keys(p).join(",") + " " + names.join(",");
}
export function main(): void {
  const q: Q = { b: "y", a: 1 };
  console.log(keys(q));
  console.log(keys({ a: 2 }));
}
