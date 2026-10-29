// The two v34 examples of docs/language-guide.md, in one program.
class ApiError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = "ApiError";
  }
}
function describe(value: unknown): string {
  if (typeof value === "string") return `text ${value}`;
  if (typeof value === "number") return `number ${value + 1}`;
  if (value === null || value === undefined) return "nothing";
  if (value instanceof Error) return `${value.name}: ${value.message}`;
  return String(value);
}
function unknownValues(): void {
  try {
    throw new ApiError("request failed", { cause: 404 });
  } catch (e) {
    console.log(describe(e));
    if (e instanceof ApiError) console.log(describe(e.cause));
  }
  console.log(describe({ id: 1 }));
}
interface Point {
  x: number;
  y: number;
}
function checkedAssertions(): void {
  const values: unknown[] = ["text", { x: 1, y: 2 }, [1, 2, 3]];
  console.log((values[0] as string).length);
  console.log((values[1] as Point).y);
  console.log((values[2] as number[]).length);
  try {
    console.log((values[0] as Point).x);
  } catch (e) {
    if (e instanceof TypeError) console.log(e.message);
  }
  const name: string | undefined = values.length > 1 ? "set" : undefined;
  console.log(name!);
}
export function main(): void {
  unknownValues();
  checkedAssertions();
}
