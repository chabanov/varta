// async/await job order: the body runs synchronously until its first await,
// every await takes one job (also for non-promise values), returning a value
// fulfills at once while returning a promise adopts it (two more jobs).
function say(text: string): void {
  console.log(text);
}
async function inner(name: string): Promise<number> {
  say(name + " inner start");
  const first = await 1;
  say(name + " inner after await " + String(first));
  return first + 1;
}
async function outer(): Promise<string> {
  say("outer start");
  const value = await inner("x");
  say("outer got " + String(value));
  return "outer done";
}
async function forwards(): Promise<number> {
  say("forwards start");
  return Promise.resolve(7);
}
async function plain(): Promise<number> {
  say("plain body");
  return 5;
}
export async function main(): Promise<void> {
  say("main start");
  outer().then((text) => say("then " + text));
  forwards().then((v) => say("forwarded " + String(v)));
  plain().then((v) => say("plain " + String(v)));
  Promise.resolve(0)
    .then(() => say("tick 1"))
    .then(() => say("tick 2"))
    .then(() => say("tick 3"))
    .then(() => say("tick 4"));
  say("main before await");
  const v = await plain();
  say("main resumed " + String(v));
  for (let i = 0; i < 3; i++) {
    await i;
    say("loop " + String(i));
  }
  say("main end");
}
