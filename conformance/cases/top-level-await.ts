const order: string[] = [];
function note(text: string): number {
  order.push(text);
  return order.length;
}
async function later(label: string): Promise<number> {
  await Promise.resolve();
  return note(label);
}
const first = note("start");
const pending = Promise.resolve().then(() => note("microtask"));
const second = await later("awaited");
const third = note("after " + String(second - first));
export async function main(): Promise<void> {
  await pending;
  note("main " + String(third));
  console.log(order.join(", "));
}
