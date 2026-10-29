// Promises held only by arrays, records, unions, unknown cells and
// dictionaries survive collections while the frames that await them wait.
interface Slot {
  name: string;
  pending: Promise<string>;
}
async function later(label: string, ticks: number): Promise<string> {
  for (let i = 0; i < ticks; i++) {
    await null;
  }
  return label + "!" + "~".repeat(ticks);
}
async function hold(): Promise<string> {
  const list: Promise<string>[] = [later("a", 2), later("b", 1)];
  const slot: Slot = { name: "s", pending: later("c", 3) };
  const either: Promise<string> | string = later("d", 1);
  const cell: unknown = later("e", 2);
  const named: Record<string, Promise<string>> = { f: later("f", 1) };
  await null;
  const all = await Promise.all(list);
  const c = await slot.pending;
  const d = await either;
  const f = await named["f"]!;
  return all[0] + all[1] + c + d + f + (typeof cell);
}
export async function main(): Promise<void> {
  const text = await hold();
  console.log(text);
  const settled = await Promise.allSettled([later("g", 2), Promise.reject<string>(new RangeError("h"))]);
  console.log(JSON.stringify(settled));
}
