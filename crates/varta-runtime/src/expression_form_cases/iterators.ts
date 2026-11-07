class Range implements Iterable<number> {
  from: number;
  to: number;
  constructor(from: number, to: number) { this.from = from; this.to = to; }
  [Symbol.iterator](): Iterator<number> {
    let current = this.from;
    const last = this.to;
    return {
      next(): IteratorResult<number> {
        if (current <= last) { return { value: current++, done: false }; }
        return { value: undefined, done: true };
      },
    };
  }
}
function* naturals(): Generator<number, void, undefined> {
  let n = 0;
  try {
    while (true) { yield n++; }
  } finally {
    console.log("naturals closed at " + String(n));
  }
}
function* twice(): Generator<string, void, undefined> { yield* ["x", "y"]; }
export function main(): void {
  const r = new Range(1, 4);
  console.log([...r].join(","));
  console.log(Array.from(r).map((x) => x * 10).join(","));
  let total = 0;
  for (const x of r) { total += x; }
  console.log(String(total));
  const [first, second, ...others] = r;
  console.log(`${first} ${second} ${others.join("|")}`);
  const [a, b] = naturals();
  console.log(`${a} ${b}`);
  const [c, , e] = "héllo";
  console.log(c + e);
  const m = new Map<string, number>([["k", 1], ["j", 2]]);
  const [[k1, v1], [k2, v2]] = m;
  console.log(`${k1}${v1}${k2}${v2}`);
  const [s1] = new Set<string>(["only"]);
  console.log(s1);
  const [t1, t2, t3 = "z"] = twice();
  console.log(t1 + t2 + t3);
  let p = 0, q = 0;
  [p, q] = naturals();
  console.log(`${p} ${q}`);
}
