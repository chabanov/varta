// `this`, captured bindings and saved locals of async methods and closures
// survive collections while their frames are suspended.
interface Entry {
  key: string;
  parts: string[];
}
class Store {
  prefix: string;
  keys: string[];
  constructor(prefix: string) {
    this.prefix = prefix;
    this.keys = [];
  }
  async add(key: string): Promise<number> {
    const entry: Entry = { key: this.prefix + key, parts: [key + "/x", key + "/y"] };
    await null;
    const junk = "z".repeat(48);
    this.keys.push(entry.key + "|" + entry.parts[1]);
    await Promise.resolve(junk.length);
    return this.keys.length;
  }
}
export async function main(): Promise<void> {
  const store = new Store("k:");
  let trail = "start";
  const label = "L" + String(store.keys.length);
  const record = async (key: string): Promise<string> => {
    const count = await store.add(key);
    trail = trail + "," + key + "=" + String(count);
    await null;
    return label + trail;
  };
  const first = record("a");
  const second = record("b");
  const firstText = await first;
  console.log(firstText);
  const secondText = await second;
  console.log(secondText);
  const keys = store.keys;
  console.log(keys[0] + " " + keys[1]);
}
