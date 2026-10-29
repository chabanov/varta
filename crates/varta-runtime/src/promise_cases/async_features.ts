// await in normalized expression positions, generic async functions, async
// functions as values, closures over frames and retries around rejections.
function say(text: string): void {
  console.log(text);
}
async function delayed<T>(value: T, label: string): Promise<T> {
  say("delayed " + label);
  await undefined;
  return value;
}
async function flaky(attempt: number): Promise<string> {
  await attempt;
  if (attempt < 3) {
    throw new Error("attempt " + String(attempt) + " failed");
  }
  return "ok on " + String(attempt);
}
async function retry(limit: number): Promise<string> {
  let lastError = "none";
  for (let attempt = 1; attempt <= limit; attempt++) {
    try {
      return await flaky(attempt);
    } catch (error) {
      if (error instanceof Error) {
        lastError = error.message;
        say("retrying after " + lastError);
      }
    }
  }
  return "gave up after " + lastError;
}
function add(a: number, b: number): number {
  return a + b;
}
async function positions(): Promise<number> {
  const sum = add(await delayed(1, "left"), await delayed(2, "right"));
  const chosen = sum > 2 ? await delayed(10, "then") : await delayed(20, "else");
  const flag = (await delayed(false, "and")) && (await delayed(true, "never"));
  const third = await delayed(3, "concat");
  const text = "n=" + String(third);
  say("text " + text + " flag " + String(flag));
  let count = 0;
  while ((await delayed(count, "test")) < 2) {
    count++;
  }
  return sum + chosen + count;
}
async function overridden(): Promise<string> {
  try {
    await 1;
    return "from try";
  } finally {
    await 2;
    say("finally ran before settlement");
  }
}
async function captured(): Promise<number> {
  let total = 0;
  const bump = (by: number): void => {
    total = total + by;
  };
  bump(1);
  await null;
  bump(2);
  await Promise.resolve(0).then(() => bump(4));
  return total;
}
export async function main(): Promise<void> {
  const producer: (label: string) => Promise<string> = (label: string) =>
    delayed("value " + label, label);
  say(await producer("p"));
  const total = await positions();
  say("positions " + String(total));
  say(await retry(5));
  say(await retry(2));
  say(await overridden());
  const sum = await captured();
  say("captured " + String(sum));
  const same: Promise<number> = Promise.resolve(9);
  say("identity " + String(Promise.resolve(same) === same));
}
