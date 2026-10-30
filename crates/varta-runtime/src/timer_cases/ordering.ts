// Timers against microtasks: main and its microtasks run before any timer;
// equal delays fire in creation order; each timer's microtasks run before the
// next timer; a timer created by a callback is ordered by its own due time;
// extra arguments reach the callback; a cleared timer never runs.
function say(text: string): void {
  console.log(text);
}
export function main(): void {
  say("main start");
  setTimeout(() => say("timeout 20"), 20);
  setTimeout(() => {
    say("timeout 0 first");
    Promise.resolve(1).then(() => say("microtask of first"));
  }, 0);
  setTimeout(() => say("timeout 0 second"), 0);
  setTimeout((label: string, count: number) => say(label + " " + String(count)), 5, "args", 2);
  const cleared = setTimeout(() => say("never"), 1);
  clearTimeout(cleared);
  clearTimeout(cleared);
  clearTimeout(undefined);
  setTimeout(() => {
    say("timeout 10");
    setTimeout(() => say("nested 10+15"), 15);
    setTimeout(() => say("nested 10+5"), 5);
  }, 10);
  Promise.resolve(0).then(() => say("microtask of main"));
  say("main end");
}
