// Timers on the event loop: main and its microtasks run first, equal delays
// fire in creation order, each timer's microtasks run before the next timer,
// a cleared timer never runs, and the sleep pattern orders awaited delays.
function sleep(ms: number): Promise<void> {
  return new Promise<void>((resolve) => setTimeout(resolve, ms));
}
async function sequence(): Promise<void> {
  await sleep(5);
  console.log("slept 5");
  await sleep(1);
  console.log("slept 1");
}
export function main(): void {
  setTimeout(() => {
    console.log("timeout 0 first");
    Promise.resolve(1).then(() => console.log("microtask of first"));
  }, 0);
  setTimeout(() => console.log("timeout 0 second"), 0);
  const cleared = setTimeout(() => console.log("never"), 1);
  clearTimeout(cleared);
  sequence();
  Promise.resolve(0).then(() => console.log("microtask of main"));
  console.log("main end");
}
