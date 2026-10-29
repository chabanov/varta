// Values that cross `await` and `yield` in async generator bodies, queued
// requests and their promises, and generators suspended at a `yield` (which
// hold no live-frame slot) stay alive across collections.
function say(text: string): void {
  console.log(text);
}
async function* words(prefix: string, n: number): AsyncGenerator<string, string, undefined> {
  let joined = prefix + ":";
  for (let i = 0; i < n; i++) {
    const piece = prefix + String(i) + "-" + String(joined.length);
    await null;
    joined = joined + piece;
    yield piece + "!";
  }
  return joined;
}
async function drain(label: string): Promise<void> {
  const it = words(label, 3);
  let r = await it.next();
  while (!r.done) {
    say(label + " " + String(r.value));
    r = await it.next();
  }
  say(label + " returned " + String(r.value));
}
async function* leaf(): AsyncGenerator<number, void, undefined> {
  yield 0;
}
async function* link(n: number, below: AsyncGenerator<number, void, undefined>): AsyncGenerator<number, void, undefined> {
  const label = "link" + String(n);
  yield label.length;
  for await (const x of below) yield x + 1;
}
// `count` generators suspended at their first `yield` at once, each held by
// the next one's frame; only the outermost is closed.
async function chain(count: number): Promise<void> {
  let g = leaf();
  let total = 0;
  for (let i = 1; i <= count; i++) {
    g = link(i, g);
    const first = await g.next();
    if (!first.done) total = total + first.value;
  }
  const closed = await g.return();
  say("chain " + String(total) + " " + String(closed.done));
}
export function main(): void {
  drain("a");
  drain("b");
  const it = words("q", 2);
  it.next().then((r) => say("q1 " + JSON.stringify(r)));
  it.next().then((r) => say("q2 " + JSON.stringify(r)));
  it.next().then((r) => say("q3 " + JSON.stringify(r)));
  chain(12);
}
