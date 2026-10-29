// yield* over async generators: values, the delegate's return value, and
// return()/throw() forwarded to the delegate.
function say(text: string): void {
  console.log(text);
}
function ticks(label: string, n: number): void {
  let p: Promise<void> = Promise.resolve();
  for (let i = 0; i < n; i++) {
    const k = i + 1;
    p = p.then(() => {
      say(label + " tick " + String(k));
    });
  }
}
async function* inner(label: string): AsyncGenerator<number, string, undefined> {
  try {
    const a = yield 1;
    say(label + " inner got " + String(a));
    yield 2;
    return label + "-r";
  } catch (e) {
    if (e instanceof Error) say(label + " inner caught " + e.message);
    yield 9;
    return label + "-caught";
  } finally {
    say(label + " inner finally");
  }
}
async function* outer(label: string): AsyncGenerator<number, string, undefined> {
  try {
    const v = yield* inner(label);
    say(label + " outer got " + v);
    yield 3;
    return "done";
  } finally {
    say(label + " outer finally");
  }
}
async function full(): Promise<void> {
  for await (const x of outer("a")) say("a " + String(x));
}
function returned(): void {
  const it = outer("b");
  it.next().then((r) => say("b1 " + JSON.stringify(r)));
  it.return("stop").then((r) => say("b2 " + JSON.stringify(r)));
  it.next().then((r) => say("b3 " + JSON.stringify(r)));
}
function thrown(): void {
  const it = outer("c");
  it.next().then((r) => say("c1 " + JSON.stringify(r)));
  it.throw(new Error("x")).then((r) => say("c2 " + JSON.stringify(r)));
  it.next().then((r) => say("c3 " + JSON.stringify(r)));
  it.next().then((r) => say("c4 " + JSON.stringify(r)));
}
export function main(): void {
  ticks("A", 14);
  full();
  ticks("B", 8);
  returned();
  ticks("C", 10);
  thrown();
}
