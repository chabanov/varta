interface Narrow {
  a: number;
  b: number;
}
interface Wide {
  b: number;
  a: number;
  c: string;
}
interface Counted {
  a: number;
  b: number;
  readonly twice: number;
  label: string;
}
let reads = 0;
function build(n: Narrow, label: string): Counted {
  return {
    get twice(): number {
      reads += 1;
      return this.a * 2;
    },
    ...n,
    label,
  };
}
function setter(n: Narrow): { a: number; b: number; total: number } {
  let stored = 0;
  return {
    ...n,
    get total(): number {
      return stored + this.a + this.b;
    },
    set total(value: number) {
      stored = value;
    },
  };
}
export function main(): void {
  const w: Wide = { b: 1, a: 2, c: "x" };
  // A view of a wider object spreads every own property of the object, in
  // its own key order, after the accessor and before the later member.
  const o = build(w, "wide");
  console.log(Object.keys(o).join(","));
  console.log(JSON.stringify(o));
  console.log(String(o.twice) + " " + String(reads));
  const p = build({ a: 5, b: 6 }, "plain");
  console.log(Object.keys(p).join(",") + " " + JSON.stringify(p));
  // The widened object keeps its accessors: reads, writes and copies.
  const s = setter(w);
  console.log(String(s.total));
  s.total = 10;
  console.log(String(s.total));
  console.log(JSON.stringify({ ...s }));
  for (const key in s) console.log(key);
  try {
    (o as { twice: number }).twice = 1;
  } catch (e) {
    console.log(e instanceof TypeError ? e.message : "other");
  }
}
