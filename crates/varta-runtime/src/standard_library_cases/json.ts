// JSON.stringify with a property list and indentation (ECMA-262 §25.5.2.1).
interface Config {
  name: string;
  tags: string[];
  nested: { a: number; b: number; c: number[] };
  empty: number[];
  none: Record<string, number>;
  note: string;
}

export function main(): void {
  const config: Config = {
    name: "svc",
    tags: ["x", "y"],
    nested: { a: 1, b: 2, c: [] },
    empty: [],
    none: {},
    note: 'quote " brace { comma , colon : backslash \\',
  };
  console.log(JSON.stringify(config, null, 2));
  console.log(JSON.stringify(config, undefined, "\t"));
  console.log(JSON.stringify(config, null, "--"));
  console.log(JSON.stringify(config, null, 0));
  console.log(JSON.stringify(config, null, 20).split("\n")[1]);
  console.log(JSON.stringify(config, null, "abcdefghijklmnop").split("\n")[1]);
  console.log(JSON.stringify(config, ["note", "name", "nested", "a", "name"]));
  console.log(JSON.stringify(config, ["nested", "b"], 1));
  const keys = [1, "b"];
  console.log(JSON.stringify({ 1: "one", b: [{ 1: 2, c: 3 }] }, keys));
  console.log(JSON.stringify([1, [2, 3]], null, 2));
  console.log(JSON.stringify("text", null, 2));
  console.log(JSON.stringify(42, null, 2));
  const indent = 3;
  console.log(JSON.stringify({ k: [true, null] }, null, indent));
}
