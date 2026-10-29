// Saved strings, records and promises survive collections forced by heavy
// allocation while their frames are suspended, across nested async calls.
function churn(units: number): number {
  // Each call allocates 10 * units bytes of garbage (UTF-16).
  return "churn".repeat(units).length;
}
interface Item {
  name: string;
  tags: string[];
}
async function holder(label: string, n: number): Promise<string> {
  const kept = label + "-" + String(n);
  const item: Item = { name: kept, tags: [kept + "/a", kept + "/b"] };
  let total = 0;
  for (let i = 0; i < n; i++) {
    await null;
    total = total + churn(400);
  }
  return item.name + ":" + item.tags[1] + ":" + String(total > 0);
}
async function chain(depth: number): Promise<number> {
  if (depth === 0) {
    return 0;
  }
  const below = await chain(depth - 1);
  return below + 1;
}
export async function main(): Promise<void> {
  const first = holder("a", 3);
  const second = holder("b", 4);
  churn(800);
  const results = [await first, await second];
  console.log(results[0] + " " + results[1]);
  let finished = 0;
  for (let round = 0; round < 8; round++) {
    const text = await holder("r", 1);
    churn(100);
    if (text.length > 0) {
      finished++;
    }
  }
  console.log("finished " + String(finished));
  const depth = await chain(10);
  console.log("depth " + String(depth));
}
