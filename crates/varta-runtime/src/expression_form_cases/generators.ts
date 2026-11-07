function* g(): Generator<number, string, number> {
  try {
    const a = yield 1;
    console.log("got " + String(a));
    yield 2;
    return "end";
  } finally {
    console.log("cleanup");
  }
}
function* counter(): Generator<number, void, undefined> {
  let i = 0;
  while (true) {
    try { yield i++; } catch (e) { console.log("caught " + (e as Error).message); }
  }
}
export function main(): void {
  const it = g();
  console.log(JSON.stringify(it.next(0)));
  console.log(JSON.stringify(it.next(5)));
  console.log(JSON.stringify(it.return("early")));
  console.log(JSON.stringify(it.next(0)));
  const it2 = g();
  it2.next(0);
  try { it2.throw(new Error("bad")); } catch (e) { console.log("thrown " + (e as Error).message); }
  const c = counter();
  console.log(String(c.next().value));
  console.log(String(c.throw(new Error("x")).value));
  console.log(String(c.next().value));
  console.log(JSON.stringify(c.return(undefined)));
  const step = g().next(0);
  console.log(Object.keys(step).join(","));
}
