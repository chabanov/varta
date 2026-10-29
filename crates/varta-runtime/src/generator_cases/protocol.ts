// next(v) delivery, return(v)/throw(e) in every generator state.
function* echo(): Generator<string, number, string> {
  const a = yield "first";
  const b = yield "got " + a;
  yield "got " + b;
  return 7;
}
function show(label: string, r: IteratorResult<string, number>): void {
  console.log(label + " " + String(r.done) + " " + String(r.value));
}
function* plain(): Generator<number, string, undefined> {
  yield 1;
  yield 2;
  return "end";
}
function report(label: string, r: IteratorResult<number, string>): void {
  console.log(label + " " + String(r.done) + " " + String(r.value));
}
export function main(): void {
  const g = echo();
  show("n0", g.next("ignored"));
  show("n1", g.next("x"));
  show("n2", g.next("y"));
  show("n3", g.next("z"));
  show("n4", g.next("w"));
  show("r5", g.return(9));
  const s = plain();
  report("start-return", s.return("early"));
  report("after", s.next());
  const t = plain();
  report("t0", t.next());
  report("t-return", t.return("mid"));
  report("t1", t.next());
  const u = plain();
  try {
    u.throw("boom");
  } catch (e) {
    console.log("not started threw " + (typeof e === "string" ? e : typeof e === "number" ? String(e) : "?"));
  }
  report("u0", u.next());
  const v = plain();
  report("v0", v.next());
  try {
    v.throw(new RangeError("at yield"));
  } catch (e) {
    console.log("uncaught in generator " + (e instanceof RangeError ? e.message : "?"));
  }
  report("v1", v.next());
  try {
    v.throw(42);
  } catch (e) {
    console.log("completed threw " + (typeof e === "string" ? e : typeof e === "number" ? String(e) : "?"));
  }
}
