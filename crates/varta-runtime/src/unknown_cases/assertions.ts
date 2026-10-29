// Checked `as` and `!` on values that satisfy them: upcasts are free and
// downcasts test the value; containers hold `unknown` fields and elements.
interface Config {
  name: string;
  retries: number;
}
interface Envelope {
  kind: string;
  payload?: unknown;
}
class Shape {
  sides: number;
  constructor(sides: number) {
    this.sides = sides;
  }
}
class Square extends Shape {
  constructor() {
    super(4);
  }
}
type Level = "low" | "high";
function read(values: unknown[], index: number): unknown {
  return values[index];
}
function first(list: string[] | undefined): string {
  return list![0] ?? "none";
}
function label(level: string): Level {
  return level as Level;
}
export function main(): void {
  const config: Config = { name: "svc", retries: 3 };
  const values: unknown[] = ["text", 2.5, config, [1, 2, 3], new Square(), null, 7n, false];
  const text = read(values, 0) as string;
  console.log(text + " " + String(text.length));
  console.log(String((read(values, 1) as number) * 2));
  const restored = read(values, 2) as Config;
  console.log(restored.name + " " + String(restored.retries));
  restored.retries = 4;
  console.log(String(config.retries));
  const numbers = read(values, 3) as number[];
  console.log(String(numbers.length) + " " + String(numbers[2]));
  const shape = read(values, 4) as Shape;
  console.log(String(shape.sides));
  const square = shape as Square;
  console.log(String(square instanceof Square));
  console.log(String(read(values, 5) as null));
  console.log(String((read(values, 6) as bigint) * 2n));
  console.log(String(read(values, 7) as boolean));
  const either = read(values, 0) as string | number;
  console.log(typeof either);
  const optional = read(values, 5) as Config | null;
  console.log(String(optional === null));
  console.log(label("high"));
  const widened = 3 as unknown;
  console.log(typeof widened);
  const viaUnknown = (config as unknown) as Config;
  console.log(viaUnknown.name);
  const envelope: Envelope = { kind: "job", payload: config };
  const payload = envelope.payload as Config;
  console.log(envelope.kind + " " + payload.name);
  const empty: Envelope = { kind: "none" };
  console.log(String(empty.payload === undefined));
  console.log(first(["a"]));
  let maybe: string | null = null;
  maybe = "set";
  console.log(maybe!);
  const nested: unknown[] = [values, "tail"];
  console.log(String(nested.length) + " " + String(nested[1]));
}
