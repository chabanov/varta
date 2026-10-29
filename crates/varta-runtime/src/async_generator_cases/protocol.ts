// Request queue order: requests made while the body runs or awaits wait in
// the queue; return() and throw() on suspended and completed generators.
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
function show(label: string, r: IteratorResult<number, number>): void {
  say(label + " " + JSON.stringify(r));
}
async function* counter(): AsyncGenerator<number, number, string> {
  say("q start");
  const a = yield 1;
  say("q got " + a);
  const b = yield 2;
  say("q got " + b);
  return 9;
}
function queued(): void {
  const it = counter();
  it.next("x").then((r) => show("r0", r));
  it.next("y").then((r) => show("r1", r));
  it.next("z").then((r) => show("r2", r));
  it.next("w").then((r) => show("r3", r));
  say("sync end");
}
async function* guarded(): AsyncGenerator<number, number, undefined> {
  try {
    yield 1;
  } finally {
    say("r finally");
    yield 5;
    say("r after 5");
  }
  return 0;
}
function returnMidway(): void {
  const it = guarded();
  it.next().then((r) => show("n1", r));
  it.return(7).then((r) => show("ret", r));
  it.next().then((r) => show("n2", r));
  it.next().then((r) => show("n3", r));
}
function returnAtStart(): void {
  const it = guarded();
  it.return(3).then((r) => show("ret", r));
  it.next().then((r) => show("n", r));
}
function throwAtStart(): void {
  const it = guarded();
  it.throw(new Error("t")).then(
    (r) => show("thr value", r),
    (e) => {
      if (e instanceof Error) say("thr " + e.message);
    },
  );
  it.next().then((r) => show("n", r));
}
async function* failing(): AsyncGenerator<number, number, undefined> {
  yield 1;
  throw new Error("bad");
}
function failingRun(): void {
  const it = failing();
  it.next().then((r) => show("b1", r));
  it.next().then(
    (r) => show("b2", r),
    (e) => {
      if (e instanceof Error) say("b2 err " + e.message);
    },
  );
  it.next().then((r) => show("b3", r));
}
async function* awaiting(): AsyncGenerator<number, number, undefined> {
  say("aw 0");
  const v = await Promise.resolve(5);
  say("aw 1 " + String(v));
  yield v;
  return await Promise.resolve(6);
}
function awaitingRun(): void {
  const it = awaiting();
  it.next().then((r) => show("a1", r));
  it.next().then((r) => show("a2", r));
  it.next().then((r) => show("a3", r));
}
export function main(): void {
  ticks("A", 5);
  queued();
  ticks("B", 5);
  returnMidway();
  ticks("C", 4);
  returnAtStart();
  throwAtStart();
  ticks("D", 3);
  failingRun();
  ticks("E", 6);
  awaitingRun();
}
