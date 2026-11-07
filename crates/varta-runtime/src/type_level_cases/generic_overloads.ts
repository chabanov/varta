// Overloads with type parameters and `this` parameters (K9 A step 4): a call
// tries the overloads in order, instantiating a generic one by inference
// from its arguments, as tsc's overload resolution does.
interface Wrap {
  (text: string): string;
  <T>(value: T): T;
}
interface Pick {
  <T>(value: T, fallback: T): T;
  (count: number): string;
}
interface Counter {
  count: number;
  label: string;
}
interface Named {
  name: string;
}
interface Describe {
  (this: Counter, prefix: string): string;
  (this: Named, prefix: string): string;
}
interface Logger {
  describe: Describe;
  count: number;
  label: string;
}
type Measure = {
  <T>(value: T): number;
  (value: string, unit: string): string;
};

function show(value: unknown): string {
  return typeof value === "string" ? `'${value}'` : String(value);
}

export function main(): void {
  const wrap = ((value: unknown) => (typeof value === "string" ? `[${value}]` : value)) as Wrap;
  const text: string = wrap("a");
  const n: number = wrap(41) + 1;
  const flag: boolean = wrap(true);
  console.log(`${text} ${n} ${flag}`);
  const pick = ((value: unknown, fallback?: unknown) =>
    fallback === undefined ? `#${String(value)}` : value ?? fallback) as Pick;
  const chosen: number = pick(3, 4);
  const label: string = pick(2);
  const word: string = pick<string>("x", "y");
  console.log(`${chosen} ${label} ${word}`);
  const describe = function (this: Counter | Named, prefix: string): string {
    const self = this;
    return "count" in self ? `${prefix}${self.label}=${self.count}` : `${prefix}${self.name}`;
  } as Describe;
  const logger: Logger = { describe, count: 2, label: "hits" };
  const named = { name: "ada", describe };
  console.log(`${logger.describe("> ")} ${named.describe("@")}`);
  const measure = ((value: unknown, unit?: string) =>
    unit === undefined ? (Array.isArray(value) ? value.length : -1) : `${String(value)}${unit}`) as Measure;
  const width: number = measure([1, 2, 3]);
  const size: string = measure("4", "px");
  console.log(`${width} ${size} ${show(measure(null))}`);
}
