// Promise jobs run after the entry point in FIFO order: reactions of one
// promise in registration order, then-chains interleaved one job per step,
// and adopting a returned promise costs two extra jobs.
export function main(): void {
  console.log("start");
  const a: Promise<number> = Promise.resolve(1);
  a.then((v) => {
    console.log("a1 " + String(v));
    return v + 1;
  }).then((v) => console.log("a2 " + String(v)));
  a.then(() => console.log("a-second"));
  Promise.resolve(10)
    .then((v) => {
      console.log("b1 " + String(v));
      return Promise.resolve(v + 1);
    })
    .then((v) => console.log("b2 " + String(v)));
  Promise.resolve(0)
    .then(() => console.log("tick 1"))
    .then(() => console.log("tick 2"))
    .then(() => console.log("tick 3"))
    .then(() => console.log("tick 4"));
  console.log("end");
}
