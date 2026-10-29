// Value graphs that every value-reading host operation observes: JSON
// stringify, diagnostics, conversions, BigInt, unions, errors and classes.
interface Meta {
  tags: string[];
  score?: number;
  owner: string | null;
}
interface Item {
  id: string;
  count: number;
  meta: Meta;
  big: bigint;
}
class Box {
  label: string;
  size: number;
  constructor(label: string, size: number) {
    this.label = label;
    this.size = size;
  }
}
class Failure extends Error {
  code: number;
  constructor(message: string, code: number) {
    super(message);
    this.name = "Failure";
    this.code = code;
  }
}
function describe(value: number | string | boolean | null | undefined): string {
  return typeof value + ":" + String(value) + ":" + String(Number(value));
}
function fail(code: number): number {
  throw new Failure("failed " + String(code), code);
}
export function main(): void {
  const item: Item = { id: "a", count: 2, meta: { tags: ["x", "y"], owner: null }, big: 12n };
  const plain = { id: item.id, count: item.count, meta: item.meta };
  console.log(JSON.stringify(plain));
  item.meta.score = 0.5;
  item.meta.tags.push("z");
  console.log(JSON.stringify(item.meta));
  try {
    console.log(JSON.stringify(item));
  } catch (e) {
    if (e instanceof TypeError) {
      console.log("bigint " + e.name);
    }
  }
  console.log(String(item.big * 3n) + " " + typeof item.big);
  const values: (number | string | boolean | null | undefined)[] = [1.5, "2", true, null, undefined];
  for (const value of values) {
    console.log(describe(value));
  }
  const mixed: (string | number[] | null)[] = ["s", [1, 2], null];
  console.log(JSON.stringify(mixed));
  console.log(Array.isArray(mixed[1]) ? "array" : "other");
  const box = new Box("crate", 3);
  console.log(JSON.stringify(box) + " " + String(box instanceof Box));
  try {
    fail(7);
  } catch (e) {
    if (e instanceof Failure) {
      console.log(e.name + " " + e.message + " " + String(e.code));
    }
    if (e instanceof Error) {
      console.log("error " + String(e instanceof RangeError));
    }
  }
}
