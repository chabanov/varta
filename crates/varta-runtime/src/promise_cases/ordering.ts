// FIFO job order: interleaved then chains, reactions registered before and
// after settlement, adoption of a returned promise (two extra jobs) and a
// value-returning handler (no extra job).
function say(text: string): void {
  console.log(text);
}
export function main(): void {
  say("script start");
  const a: Promise<number> = Promise.resolve(1);
  const b: Promise<number> = Promise.resolve(2);
  a.then((v) => {
    say("a1 " + String(v));
    return v * 10;
  }).then((v) => {
    say("a2 " + String(v));
  });
  b.then((v) => {
    say("b1 " + String(v));
    return Promise.resolve(v * 10);
  }).then((v) => {
    say("b2 " + String(v));
  });
  a.then(() => say("a-second"));
  let resolveLater: (value: string) => void = (value: string) => {
    say("placeholder " + value);
  };
  const pending = new Promise<string>((resolve) => {
    say("executor runs synchronously");
    resolveLater = resolve;
  });
  pending.then((value) => say("pending got " + value));
  pending.then((value) => say("pending again " + value));
  Promise.resolve(3)
    .then((v) => v + 1)
    .then((v) => v + 1)
    .then((v) => {
      say("c3 " + String(v));
      resolveLater("late");
      say("after resolveLater");
    });
  say("script end");
}
