// Conditional types: distribution, infer (with constraints), recursion.
type IsString<T> = T extends string ? "yes" : "no";
type NonString<T> = T extends string ? never : T;
type Flatten<T> = T extends (infer U)[] ? U : T;
type Unwrap<T> = T extends Promise<infer V> ? V : T;
type First<T extends unknown[]> = T extends [infer H, ...unknown[]] ? H : never;
type Last<T extends unknown[]> = T extends [...unknown[], infer L] ? L : never;
type Tail<T extends unknown[]> = T extends [unknown, ...infer R] ? R : never;
type Reverse<T extends unknown[]> = T extends [infer H, ...infer R] ? [...Reverse<R>, H] : [];
type Length<T extends unknown[]> = T["length"];
type ToNumber<T> = T extends `${infer N extends number}` ? N : never;
type Result<F> = F extends (...args: never[]) => infer R ? R : never;
type FirstArg<F> = F extends (first: infer A) => unknown ? A : never;
type Leading<T> = T extends [infer S extends string, ...unknown[]] ? S : "none";
type Wrapped<T> = [T] extends [string] ? "string" : "other";
type Field<T> = T extends { id: infer I } ? I : "no id";

function label(value: IsString<"a">): string {
  return value;
}

export function main(): void {
  const yes: IsString<string> = "yes";
  const no: IsString<number> = "no";
  const both: IsString<string | number>[] = ["yes", "no"];
  console.log(`${label("yes")} ${yes} ${no} ${both.join("/")}`);
  const rest: NonString<string | number | boolean> = 5;
  const flag: NonString<string | number | boolean> = true;
  console.log(`${rest} ${flag}`);
  const element: Flatten<number[]> = 3;
  const plain: Flatten<string> = "plain";
  const settled: Unwrap<Promise<string>> = "settled";
  console.log(`${element} ${plain} ${settled}`);
  const head: First<[number, string]> = 1;
  const last: Last<[number, string, boolean]> = false;
  const tail: Tail<[number, string, boolean]> = ["two", true];
  console.log(`${head} ${last} ${tail[0]} ${tail[1]} ${tail.length}`);
  const reversed: Reverse<[1, 2, 3]> = [3, 2, 1];
  const length: Length<[1, 2, 3]> = 3;
  console.log(`${reversed.join(",")} ${length}`);
  const parsed: ToNumber<"42"> = 42;
  const returned: Result<() => boolean> = true;
  const argument: FirstArg<(text: string) => void> = "arg";
  console.log(`${parsed + 1} ${returned} ${argument}`);
  const leading: Leading<["x", 1]> = "x";
  const none: Leading<[1, "x"]> = "none";
  const wrapped: Wrapped<string | number> = "other";
  const single: Wrapped<"s"> = "string";
  console.log(`${leading} ${none} ${wrapped} ${single}`);
  const id: Field<{ id: number; name: string }> = 7;
  const missing: Field<{ name: string }> = "no id";
  console.log(`${id} ${missing}`);
}
