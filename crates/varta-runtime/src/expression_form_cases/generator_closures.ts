interface Counter {
  start: number;
  items(limit: number): Generator<number, void, unknown>;
}
interface Plain {
  pairs(): Generator<string, number, unknown>;
}
function collect(gen: Generator<number, void, unknown>): string {
  const seen: number[] = [];
  for (const value of gen) seen.push(value);
  return seen.join(",");
}
function make(step: number): () => Generator<number, void, unknown> {
  let calls = 0;
  return function* (): Generator<number, void, unknown> {
    calls += 1;
    for (let i = 0; i < 3; i++) yield i * step + calls;
  };
}
function checked(limit: number): Generator<number, string, number> {
  const gen = function* named(n: number): Generator<number, string, number> {
    let total = 0;
    while (n > 0) {
      const input = yield total;
      total += input;
      n -= 1;
    }
    return "total " + total;
  };
  return gen(limit);
}
export function main(): void {
  // A `function*` expression closes over its environment, and its body
  // starts at the first `next`.
  const f = make(10);
  console.log(collect(f()));
  console.log(collect(f()));
  let trace = "";
  const lazy = function* (): Generator<string, void, unknown> {
    trace += "started;";
    yield "a";
    trace += "resumed;";
  };
  const g = lazy();
  console.log(trace === "" ? "not started" : trace);
  console.log(JSON.stringify(g.next()));
  console.log(trace);
  console.log(JSON.stringify(g.next()));
  console.log(trace);
  // `next` arguments, `return` and the named binding.
  const c = checked(2);
  console.log(JSON.stringify(c.next(100)));
  console.log(JSON.stringify(c.next(5)));
  console.log(JSON.stringify(c.next(7)));
  console.log(JSON.stringify(c.next(1)));
  // Generator methods of object literals, with and without `this`.
  const counter: Counter = {
    start: 4,
    *items(limit: number): Generator<number, void, unknown> {
      for (let i = this.start; i < this.start + limit; i++) yield i;
    },
  };
  console.log(collect(counter.items(3)));
  const other = { ...counter, start: 10 };
  console.log(collect(other.items(2)));
  let suffix = "!";
  const plain: Plain = {
    *pairs(): Generator<string, number, unknown> {
      yield "x" + suffix;
      suffix = "?";
      yield "y" + suffix;
      return 2;
    },
  };
  const p = plain.pairs();
  console.log(JSON.stringify([p.next(), p.next(), p.next(), p.next()]));
  console.log(suffix);
  // A generator method's receiver is taken at the call; its body reads it
  // at the first `next`.
  const items = counter.items;
  const detached = items(1);
  console.log("created");
  try {
    detached.next();
    console.log("no throw");
  } catch (e) {
    console.log(e instanceof TypeError ? "TypeError at next" : "other");
  }
  // try/finally inside a generator closure, closed by `return`.
  const closing = function* (): Generator<number, void, unknown> {
    try {
      yield 1;
      yield 2;
    } finally {
      trace = "closed";
    }
  };
  const h = closing();
  console.log(JSON.stringify(h.next()));
  console.log(JSON.stringify(h.return(undefined)));
  console.log(trace);
  // Parameter defaults are evaluated at the call.
  let defaults = 0;
  const withDefault = function* (n: number = ++defaults): Generator<number, void, unknown> {
    yield n;
  };
  const d = withDefault();
  console.log(String(defaults));
  console.log(JSON.stringify([...d, ...withDefault(7)]));
  // Spread of a generator closure's object.
  const spread = function* (n: number): Generator<number, void, unknown> {
    for (let i = 0; i < n; i++) yield i * i;
  };
  console.log(JSON.stringify([...spread(4)]));
}
