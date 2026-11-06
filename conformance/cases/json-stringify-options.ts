interface Value {
  name: string;
  tags: string[];
  empty: number[];
  nested: { n: number };
}
export function main(): void {
  const value: Value = { name: "x", tags: ["a", "b"], empty: [], nested: { n: 1 } };
  for (const line of JSON.stringify(value, null, 2).split("\n")) {
    console.log(line);
  }
  console.log(JSON.stringify(value, ["nested", "name", "n"]));
  console.log(JSON.stringify([1, [2]], null, "->").split("\n").join(" "));
}
