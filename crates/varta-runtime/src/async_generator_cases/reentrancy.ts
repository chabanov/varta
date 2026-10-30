// Requests from inside the running body wait in the queue; throw() into a
// suspended generator reaches its catch; unhandled rejections stay pending
// until a handler arrives in the checkpoint.
function say(text: string): void {
  console.log(text);
}
async function* reentrant(hook: () => void): AsyncGenerator<number, void, undefined> {
  say("body start");
  hook();
  say("body after hook");
  yield 1;
  say("body after 1");
  yield 2;
  say("body end");
}
async function* catcher(): AsyncGenerator<string, string, undefined> {
  try {
    yield "first";
  } catch (e) {
    if (e instanceof Error) say("caught " + e.message);
    yield "recovered";
  }
  return "end";
}
export function main(): void {
  let hook = (): void => {};
  const it = reentrant(() => hook());
  hook = () => {
    it.next().then((r) => say("inner " + JSON.stringify(r)));
  };
  it.next().then((r) => say("outer " + JSON.stringify(r)));
  it.next().then((r) => say("third " + JSON.stringify(r)));
  const c = catcher();
  c.next().then((r) => say("c1 " + JSON.stringify(r)));
  c.throw(new Error("injected")).then((r) => say("c2 " + JSON.stringify(r)));
  c.next().then((r) => say("c3 " + JSON.stringify(r)));
  c.throw(new RangeError("late")).then(
    (r) => say("c4 " + JSON.stringify(r)),
    (e) => {
      if (e instanceof Error) say("c4 rejected " + e.name + " " + e.message);
    },
  );
}
