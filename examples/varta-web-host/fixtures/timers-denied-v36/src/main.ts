export function main(): void {
  console.log("start");
  setTimeout(() => console.log("t30"), 30);
  setTimeout(() => console.log("t10"), 10);
  const cancelled = setTimeout(() => console.log("never"), 20);
  clearTimeout(cancelled);
  setTimeout(() => console.log("t10b"), 10);
  setTimeout(() => console.log("t0 becomes 1ms"), 0);
  let ticks = 0;
  let interval = 0;
  interval = setInterval(() => {
    ticks = ticks + 1;
    console.log("tick");
    console.log(ticks);
    if (ticks === 3) clearInterval(interval);
  }, 7);
  console.log("end");
}
