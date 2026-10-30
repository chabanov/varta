// Timer-driven promises: the sleep pattern, a retry loop with exponential
// backoff between attempts (the uarp-sdk `backoffDelay`/`streamBackoffDelay`
// shape with an explicit random source), intervals cleared by their own
// callback and by another timer, and async timer callbacks.
function sleep(ms: number): Promise<void> {
  return new Promise<void>((resolve) => setTimeout(resolve, ms));
}
function delayed(value: string, ms: number): Promise<string> {
  return new Promise<string>((resolve) => {
    setTimeout(() => resolve(value), ms);
  });
}
function streamBackoffDelay(
  attempt: number,
  baseIntervalMs: number,
  maxDelayMs: number,
  random: () => number,
): number {
  const exponential = baseIntervalMs * 2 ** Math.max(attempt - 1, 0);
  const maxSleep = Math.min(maxDelayMs, exponential);
  const half = Math.max(maxSleep / 2, 1);
  return Math.round(half + random() * half);
}
function flaky(attempt: number): number {
  if (attempt < 3) {
    throw new Error("attempt " + String(attempt + 1) + " failed");
  }
  return attempt + 1;
}
async function retry(): Promise<number> {
  let attempt = 0;
  while (attempt < 5) {
    try {
      return flaky(attempt);
    } catch (error) {
      attempt = attempt + 1;
      const wait = streamBackoffDelay(attempt, 100, 250, () => 0.5);
      console.log((error as Error).message + ", retry in " + String(wait));
      await sleep(wait);
    }
  }
  throw new Error("retries exhausted");
}
async function ticker(): Promise<void> {
  let ticks = 0;
  // A closure cannot capture a binding before its initialization in Varta,
  // so the interval's own id is assigned to an initialized binding.
  let id = 0;
  id = setInterval(() => {
    ticks = ticks + 1;
    console.log("tick " + String(ticks));
    if (ticks === 3) {
      clearInterval(id);
    }
  }, 40);
  await sleep(200);
  console.log("ticks " + String(ticks));
}
export async function main(): Promise<void> {
  console.log("start");
  const slow = delayed("slow", 30);
  const fast = delayed("fast", 10);
  const first = await slow;
  const second = await fast;
  console.log(first + "," + second);
  const result = await retry();
  console.log("result " + String(result));
  await ticker();
  const other = setInterval(() => console.log("other interval"), 15);
  setTimeout(() => clearInterval(other), 50);
  setTimeout(async () => {
    console.log("async callback");
    await sleep(5);
    console.log("async callback resumed");
  }, 60);
  await sleep(100);
  console.log("end");
}
