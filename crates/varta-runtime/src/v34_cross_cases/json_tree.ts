// A recursive JsonValue tree built across awaits, held in `unknown` and in
// dictionaries while churn forces collections; serialization pins the graph.
type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
function churn(): number {
  // Each call allocates about 16 KB of garbage strings; with the live tree it forces a collection.
  let text = "x";
  for (let i = 0; i < 13; i++) {
    text = text + text;
  }
  return text.length;
}
async function tree(depth: number): Promise<JsonValue> {
  await null;
  if (depth === 0) return depth % 2 === 0 ? "leaf" : null;
  const children: JsonValue[] = [];
  for (let i = 0; i < 2; i++) {
    const child = await tree(depth - 1);
    children.push(child);
  }
  const node: { [key: string]: JsonValue } = { depth: depth, children: children };
  node["even"] = depth % 2 === 0;
  return node;
}
function describe(value: unknown): string {
  if (typeof value === "string") return `string ${value}`;
  if (typeof value === "number") return `number ${value}`;
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  if (typeof value === "object") return "object";
  return typeof value;
}
export async function main(): Promise<void> {
  const root = await tree(2);
  const held: unknown = root;
  await null;
  churn();
  console.log(describe(held));
  console.log(JSON.stringify(held));
  const leaf: unknown = await tree(0);
  console.log(describe(leaf));
  const dictionary: Record<string, unknown> = { tree: held, leaf: leaf, count: 2 };
  await null;
  churn();
  console.log(JSON.stringify(Object.keys(dictionary)));
  console.log(describe(dictionary["count"]));
}
