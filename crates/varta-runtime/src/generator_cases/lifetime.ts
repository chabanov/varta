// Generator frames belong to their objects: more than the live-frame table
// can hold stay suspended at once, dropped ones are collected, and saved
// strings survive collections forced by heavy allocation.
function* tiny(n: number): Generator<number> {
  yield n;
  yield n + 1;
}
function* labelled(prefix: string, n: number): Generator<string> {
  const kept = prefix + "-" + String(n);
  for (let i = 0; i < n; i++) {
    let junk = "";
    for (let j = 0; j < 8; j++) {
      junk = junk + "churn-" + String(j);
    }
    yield kept + ":" + String(i) + ":" + String(junk.length);
  }
}
function nested(depth: number): number {
  const g = tiny(depth);
  const first = g.next().value;
  const inner = depth === 0 ? 0 : nested(depth - 1);
  const second = g.next().value;
  return (first === depth ? 1 : 0) + (second === depth + 1 ? 1 : 0) + inner;
}
export function main(): void {
  let dropped = 0;
  for (let round = 0; round < 100; round++) {
    const g = tiny(round);
    if (!g.next().done) {
      dropped++;
    }
  }
  console.log("dropped " + String(dropped));
  console.log("nested " + String(nested(66)));
  const a = labelled("a", 3);
  const b = labelled("b", 3);
  const seen: string[] = [];
  for (let i = 0; i < 3; i++) {
    seen.push(String(a.next().value));
    seen.push(String(b.next().value));
  }
  console.log(seen[0] + " " + seen[3] + " " + seen[5]);
}
