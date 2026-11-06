// Every built-in utility type over resolved types.
interface Todo {
  title: string;
  done: boolean;
  note?: string;
}
class Point {
  x: number;
  y: number;
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }
}
function greet(this: Todo, greeting: string): string {
  return `${greeting} ${this.title}`;
}
function sum(a: number, b?: number, ...rest: number[]): number {
  let total = a + (b ?? 0);
  for (const value of rest) {
    total += value;
  }
  return total;
}
async function load(): Promise<string> {
  return "loaded";
}

type Kinds = "a" | "b" | "c" | null | undefined;

export async function main(): Promise<void> {
  const partial: Partial<Todo> = { done: true };
  const required: Required<Todo> = { title: "r", done: false, note: "n" };
  const frozen: Readonly<Todo> = { title: "f", done: true };
  console.log(`${partial.title} ${partial.done} ${required.note} ${frozen.title}`);
  const picked: Pick<Todo, "title" | "note"> = { title: "p" };
  const omitted: Omit<Todo, "done" | "note"> = { title: "o" };
  console.log(`${picked.title} ${picked.note} ${Object.keys(omitted).join(",")}`);
  const record: Record<"x" | "y", number> = { x: 1, y: 2 };
  const dictionary: Record<string, boolean> = { on: true };
  console.log(`${record.x + record.y} ${dictionary["on"]}`);
  const excluded: Exclude<Kinds, "a" | null | undefined>[] = ["b", "c"];
  const extracted: Extract<Kinds, "a" | "z"> = "a";
  const present: NonNullable<Kinds>[] = ["a", "c"];
  console.log(`${excluded.join("")} ${extracted} ${present.join("")}`);
  const result: ReturnType<typeof sum> = sum(1, 2, 3, 4);
  const args: Parameters<typeof sum> = [5, 6, 7];
  const fewer: Parameters<typeof sum> = [8];
  console.log(`${result} ${sum(args[0], args[1], args[2])} ${args.length} ${fewer.length}`);
  const coordinates: ConstructorParameters<typeof Point> = [3, 4];
  const point: InstanceType<typeof Point> = new Point(coordinates[0], coordinates[1]);
  console.log(`${point.x * point.y}`);
  const self: ThisParameterType<typeof greet> = { title: "t", done: false };
  const unbound: OmitThisParameter<(this: Todo, greeting: string) => string> = (greeting: string) => greeting;
  console.log(`${greet.call(self, "hi")} ${unbound("plain")}`);
  const settled: Awaited<ReturnType<typeof load>> = await load();
  const nested: Awaited<Promise<number> | number> = 9;
  const inferred: NoInfer<number> = 10;
  console.log(`${settled} ${nested + inferred}`);
  const upper: Uppercase<"abc"> = "ABC";
  const lower: Lowercase<"ABC"> = "abc";
  const capital: Capitalize<"word"> = "Word";
  const small: Uncapitalize<"Word"> = "word";
  console.log(`${upper} ${lower} ${capital} ${small}`);
}
