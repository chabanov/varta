class Failure extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = "Failure";
  }
}
function describe(value: unknown): string {
  if (typeof value === "string") return "string " + value;
  if (typeof value === "number") return "number " + String(value + 1);
  if (value === null) return "null";
  if (value instanceof Failure) return "failure " + value.message + " " + describe(value.cause);
  if (value instanceof Error) return "error " + value.message;
  return typeof value;
}
function attempt(kind: number): string {
  try {
    if (kind === 0) throw new Failure("outer", { cause: "inner" });
    if (kind === 1) throw new RangeError("range");
    throw 41;
  } catch (e) {
    return describe(e);
  }
}
export function main(): void {
  const values: unknown[] = ["a", 1, null, undefined, { k: 1 }];
  for (const value of values) console.log(describe(value));
  for (let kind = 0; kind < 3; kind++) console.log(attempt(kind));
  const held: unknown = values[0];
  console.log(String(held === "a"));
  console.log(`${held} ${values[1]}`);
}
