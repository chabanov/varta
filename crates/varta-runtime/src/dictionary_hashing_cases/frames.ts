// V38 hashed dictionaries held by closures, generator frames and async
// frames across suspensions: an indexed dictionary with holes stays valid in
// every capture and frame slot. Expected lines are Node 24's.
function* walk(d: Record<string, number>): Generator<string, void, undefined> {
  for (const key of Object.keys(d)) {
    yield key + "=" + String(d[key]);
    delete d[key];
  }
}
async function settle(d: Record<string, number>): Promise<number> {
  await null;
  delete d["k2"];
  await null;
  d["late"] = 1;
  return Object.keys(d).length;
}
export function main(): void {
  const d: Record<string, number> = {};
  for (let i = 0; i < 24; i++) {
    d["k" + String(i)] = i;
  }
  delete d["k0"];
  delete d["k9"];
  const read = (key: string): number => d[key] ?? -1;
  console.log(String(read("k1")) + " " + String(read("k0")) + " " + String(read("k23")));
  settle(d).then((count: number): void => {
    console.log("settled " + String(count) + " " + String(read("late")) + " " + String(read("k2")));
    const small: Record<string, number> = { a: 1, b: 2, c: 3 };
    let text = "";
    for (const step of walk(small)) {
      text += step + ";";
    }
    console.log(text + " " + String(Object.keys(small).length));
  });
  console.log("sync " + String(Object.keys(d).length));
}
