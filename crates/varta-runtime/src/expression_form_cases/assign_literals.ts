interface Wide {
  b: number;
  a: number;
  c: string;
}
interface Narrow {
  a: number;
  b: number;
}
const trace: string[] = [];
function fresh(n: Narrow): string {
  return JSON.stringify(Object.assign({}, n));
}
function merged(n: Narrow, extra: { d: boolean } | undefined): string {
  return JSON.stringify(Object.assign({ z: 0, a: -1 }, n, extra));
}
export function main(): void {
  const w: Wide = { b: 1, a: 2, c: "x" };
  console.log(fresh(w));
  console.log(merged(w, { d: true }));
  console.log(merged({ a: 5, b: 6 }, undefined));
  const counted: Narrow = {
    get a(): number {
      trace.push("a");
      return 7;
    },
    b: 8,
  };
  console.log(fresh(counted) + " " + trace.join(","));
}
