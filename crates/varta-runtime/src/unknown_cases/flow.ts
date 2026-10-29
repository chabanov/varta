// `unknown` across closures, generators, records, arrays, JSON and logging.
interface Entry {
  key: string;
  value: unknown;
}
function capture(value: unknown): () => string {
  return () => (typeof value === "number" ? "number " + String(value) : "other " + typeof value);
}
function* walk(items: unknown[]): Generator<unknown, string, undefined> {
  let count = 0;
  for (const item of items) {
    count++;
    yield item;
  }
  return "walked " + String(count);
}
function classify(entries: Entry[]): string {
  let text = "";
  for (const entry of entries) {
    const value = entry!.value;
    if (typeof value === "string") {
      text = text + entry!.key + "=" + value + ";";
    } else if (Array.isArray(value)) {
      text = text + entry!.key + "=array;";
    } else if (typeof value === "object") {
      text = text + entry!.key + "=" + (value === null ? "null" : "object") + ";";
    } else {
      text = text + entry!.key + "=" + typeof value + ";";
    }
  }
  return text;
}
export function main(): void {
  console.log(capture(4)());
  console.log(capture("x")());
  const it = walk([1, "two", null]);
  let step = it.next();
  while (!step.done) {
    console.log("yield " + String(step.value));
    step = it.next();
  }
  console.log(String(step.value));
  const entries: Entry[] = [
    { key: "a", value: "text" },
    { key: "b", value: [1] },
    { key: "c", value: null },
    { key: "d", value: { inner: true } },
    { key: "e", value: 3 },
  ];
  console.log(classify(entries));
  const row: unknown = { id: 7, tags: ["x", "y"], extra: undefined };
  console.log(JSON.stringify(row));
  console.log(JSON.stringify([1, "a", null] as unknown));
  console.log(String(JSON.stringify(undefined as unknown)));
  console.log(JSON.stringify({ when: entries[4]!.value }));
  const values: unknown[] = [];
  for (let i = 0; i < 40; i++) {
    values.push(i % 2 === 0 ? { index: i, label: "n" + String(i) } : "s" + String(i));
  }
  let total = 0;
  for (const value of values) {
    if (typeof value === "string") total += value.length;
  }
  console.log(String(values.length) + " " + String(total));
}
