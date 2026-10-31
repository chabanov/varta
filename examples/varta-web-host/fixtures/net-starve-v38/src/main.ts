// An application instance kept busy by a 1 ms interval must still deliver a
// completion (embedding.md "Long-lived application instance (v38)": a sustained
// stream of timers cannot starve it). Each tick works longer than the interval,
// and so does main after starting both, so a timer is due whenever a turn ends. The interval stops itself after
// GIVE_UP_TICKS wall-clock ticks, so a starved completion shows as the interval
// stopping first. The test rewrites the grant to the server's port.
const TICK_MS = 1;
const GIVE_UP_TICKS = 500;
const WORK_PER_TICK = 2000000;

function work(): void {
  let sum = 0;
  for (let i = 0; i < WORK_PER_TICK; i += 1) sum += i % 7;
  if (sum < 0) console.log("unreachable");
}

export function main(): void {
  const base = varta.env.get("BASE_URL");
  let ticks = 0;
  let answered = false;
  const ticker = setInterval(() => {
    ticks += 1;
    work();
    if (ticks < GIVE_UP_TICKS) return;
    clearInterval(ticker);
    if (!answered) console.log("the interval stopped first");
  }, TICK_MS);
  fetch(base + "/json").then((r) => {
    answered = true;
    clearInterval(ticker);
    console.log("answered " + String(r.status));
  });
  work();
}
