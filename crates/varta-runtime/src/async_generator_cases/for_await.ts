// for await over arrays, sync generators and async generators: job counts
// per step, closing on break and throw, and rejected values.
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
async function arrays(): Promise<void> {
  for await (const x of [1, 2]) say("arr " + String(x));
  say("arr done");
  const ps: Promise<number>[] = [Promise.resolve(3), Promise.resolve(4)];
  for await (const x of ps) {
    say("ps " + String(x));
    break;
  }
  say("ps after");
}
function* values(): Generator<Promise<number>, void, undefined> {
  try {
    yield Promise.resolve(1);
    yield Promise.reject(new Error("boom"));
    yield Promise.resolve(3);
  } finally {
    say("sg finally");
  }
}
async function syncGenerators(): Promise<void> {
  try {
    for await (const x of values()) say("sg " + String(x));
  } catch (e) {
    if (e instanceof Error) say("sg caught " + e.message);
  }
  for await (const x of values()) {
    say("sg2 " + String(x));
    break;
  }
  say("sg2 after");
}
async function* source(): AsyncGenerator<number, void, undefined> {
  try {
    say("g start");
    yield 1;
    yield 2;
  } finally {
    say("g finally");
  }
}
async function asyncGenerators(): Promise<void> {
  for await (const x of source()) {
    say("ag " + String(x));
    break;
  }
  say("ag after");
  try {
    for await (const x of source()) {
      say("ag2 " + String(x));
      throw new Error("body");
    }
  } catch (e) {
    if (e instanceof Error) say("ag2 caught " + e.message);
  }
  let total = 0;
  for await (const x of source()) total += x;
  say("total " + String(total));
}
export function main(): void {
  ticks("A", 8);
  arrays();
  ticks("B", 8);
  syncGenerators();
  ticks("C", 10);
  asyncGenerators();
}
