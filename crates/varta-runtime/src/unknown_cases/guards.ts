// `unknown` values: widening, typeof/null/undefined/literal guards, equality,
// truthiness, logical operators and String conversion.
interface Point {
  x: number;
  y: number;
}
function describe(value: unknown): string {
  if (typeof value === "string") return "string:" + value + ":" + String(value.length);
  if (typeof value === "number") return "number:" + String(value * 2);
  if (typeof value === "boolean") return "boolean:" + String(!value);
  if (typeof value === "bigint") return "bigint:" + String(value + 1n);
  if (value === null) return "null";
  if (value === undefined) return "undefined";
  return "other:" + typeof value;
}
function mode(value: unknown): string {
  if (value === "fast") {
    const chosen: "fast" | "slow" = value;
    return "literal " + chosen;
  }
  return "no literal";
}
function truth(value: unknown): string {
  return value ? "truthy" : "falsy";
}
function fallback(value: unknown): unknown {
  return value ?? "default";
}
export function main(): void {
  const point: Point = { x: 1, y: 2 };
  const values: unknown[] = ["ab", 21, true, 9n, null, undefined, point, [1, 2]];
  for (const value of values) {
    console.log(describe(value) + " " + typeof value + " " + truth(value));
  }
  console.log(mode("fast"));
  console.log(mode("slow"));
  console.log(mode(1));
  for (const value of [0, "", NaN, -0, 0n, false, "x", 1]) {
    console.log(String(value) + " " + truth(value));
  }
  let held: unknown = point;
  console.log(String(held === point));
  console.log(String(held !== point));
  held = { x: 1, y: 2 };
  console.log(String(held === point));
  held = "ab";
  console.log(String(held === "ab"));
  console.log(String(held === "abc"));
  console.log(String(held == null));
  held = undefined;
  console.log(String(held == null));
  console.log(String(held === undefined));
  console.log(String(fallback(undefined)));
  console.log(String(fallback(null)));
  console.log(String(fallback(0)));
  const first: unknown = "";
  console.log(String(first || "empty"));
  console.log(String(first && "never"));
  console.log(String(12));
  console.log(String(held));
  console.log(String(point as unknown));
  console.log(String(new TypeError("bad") as unknown));
  let text: unknown = 5;
  text = "now text";
  if (typeof text === "string") {
    console.log(text.slice(0, 3));
  }
  const parts: unknown[] = [1, null, point, 2n];
  console.log(`tpl ${parts[0]} ${parts[1]} ${parts[2]} ${parts[3]} ${parts[9]}`);
}
