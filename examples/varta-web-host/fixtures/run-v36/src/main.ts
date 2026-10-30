class Counter {
  count = 0;
  increment(by: number): number {
    this.count += by;
    return this.count;
  }
}
async function delayed(value: number): Promise<number> {
  await null;
  return value * 2;
}
async function work(): Promise<void> {
  const counter = new Counter();
  const steps: number[] = [1, 2, 3];
  for (let i = 0; i < steps.length; i++) counter.increment(steps[i] ?? 0);
  console.log(counter.count);
  const doubled = await delayed(21);
  console.log(doubled);
  try {
    throw new Error("boom");
  } catch (error) {
    console.log("caught");
  }
  console.log(Math.sin(1));
  console.log(0.1 + 0.2);
  console.log("😀 ok".charCodeAt(0));
}
export async function main(): Promise<void> {
  console.log("before");
  await work();
  console.log("after");
}
