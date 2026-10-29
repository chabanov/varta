// yield inside try/catch/finally; return()/throw() completion ordering.
function join(items: string[], sep: string): string {
  let text = "";
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    if (item !== undefined) {
      text = i === 0 ? item : text + sep + item;
    }
  }
  return text;
}
function* guarded(trace: string[]): Generator<string> {
  try {
    trace.push("enter");
    yield "a";
    trace.push("after a");
    yield "b";
  } finally {
    trace.push("cleanup");
    yield "in finally";
    trace.push("finally done");
  }
  trace.push("unreachable after return");
}
function* catching(): Generator<string> {
  try {
    yield "try";
  } catch (e) {
    yield "caught " + (typeof e === "string" ? e : typeof e === "number" ? String(e) : "?");
    yield "still in catch";
  }
  yield "after";
}
function* rethrow(trace: string[]): Generator<number> {
  try {
    try {
      yield 1;
    } finally {
      trace.push("inner finally");
    }
  } catch (e) {
    trace.push("outer catch " + (typeof e === "string" ? e : typeof e === "number" ? String(e) : "?"));
    yield 2;
  } finally {
    trace.push("outer finally");
  }
}
function line(r: IteratorResult<string>): string {
  return String(r.done) + ":" + String(r.value);
}
export function main(): void {
  const trace: string[] = [];
  const g = guarded(trace);
  console.log(line(g.next()));
  console.log(line(g.return()));
  console.log(line(g.next()));
  console.log(line(g.next()));
  console.log(join(trace, ","));
  const c = catching();
  console.log(line(c.next()));
  console.log(line(c.throw("x")));
  console.log(line(c.next()));
  console.log(line(c.next()));
  console.log(line(c.next()));
  const c2 = catching();
  console.log(line(c2.next()));
  console.log(line(c2.throw("y")));
  console.log(line(c2.return()));
  console.log(line(c2.next()));
  const events: string[] = [];
  const r = rethrow(events);
  const first = r.next();
  console.log(String(first.done) + ":" + String(first.value));
  const second = r.throw("e1");
  console.log(String(second.done) + ":" + String(second.value));
  const third = r.next();
  console.log(String(third.done) + ":" + String(third.value));
  console.log(join(events, ","));
}
