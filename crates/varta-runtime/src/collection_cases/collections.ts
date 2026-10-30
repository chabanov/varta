interface Options {
  events?: Set<string>;
}
function pair(): [string, number] {
  return ["k", 7];
}
export function main(): void {
  const seen = new Set<string>();
  seen.add("a").add("b");
  seen.add("a");
  console.log(`${seen.size} ${seen.has("a")} ${seen.has("z")}`);
  for (const v of seen) console.log(v);
  const retry = new Set([408, 429, 500]);
  console.log(`${retry.has(429)} ${retry.has(404)} ${retry.size}`);
  const nan = new Set<number>();
  nan.add(NaN);
  nan.add(NaN);
  nan.add(-0);
  nan.add(0);
  console.log(`${nan.size} ${nan.has(NaN)} ${nan.has(0)}`);
  const t = pair();
  console.log(`${t[0]} ${t[1]} ${t.length}`);
  const [k, n] = t;
  console.log(`${k}=${n}`);
  const m = new Map<string, number>();
  m.set("x", 1).set("y", 2);
  m.set("x", 3);
  console.log(`${m.size} ${m.get("x")} ${m.get("q")}`);
  for (const [key, value] of m) console.log(`${key}:${value}`);
  m.delete("x");
  for (const key of m.keys()) console.log(key);
  const opts: Options = { events: new Set(["done"]) };
  const events = opts.events ?? new Set();
  console.log(`${events.has("done")}`);
  const list = [...seen];
  console.log(list.join("+"));
  const m2 = new Map([["p", true], ["q", false]] as [string, boolean][]);
  console.log(`${m2.get("q")} ${m2.size}`);
  let total = 0;
  m.forEach((value) => {
    total += value;
  });
  console.log(`${total}`);
  seen.clear();
  console.log(`${seen.size}`);
}
