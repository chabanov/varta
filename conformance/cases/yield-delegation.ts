function* inner(trace: string[]): Generator<number, string> {
  try {
    yield 1;
    yield 2;
  } catch (e) {
    trace.push("inner caught " + String(e));
    yield 3;
  } finally {
    trace.push("inner finally");
  }
  return "inner done";
}
function* outer(trace: string[]): Generator<number, string> {
  const result = yield* inner(trace);
  trace.push(result);
  yield* [10, 20];
  return "outer done";
}
export function main(): void {
  const trace: string[] = [];
  console.log([...outer(trace)].join(","));
  const g = outer(trace);
  console.log(String(g.next().value));
  console.log(String(g.throw("boom").value));
  console.log(String(g.next().value));
  const h = outer(trace);
  h.next();
  const closed = h.return("early");
  console.log(String(closed.value) + " " + String(closed.done));
  console.log(trace.join("; "));
}
