// Pattern template literal types and intrinsic string types over string:
// literals and template expressions are checked against the pattern.
type Handler = `on${string}`;
type Px = `${number}px`;
type Id = `id-${bigint}`;
type Loud = Uppercase<string>;
type Title = Capitalize<string>;
type Event = Uppercase<`on${string}`>;
type Mixed = "none" | `#${string}`;
type OnlyColor = Exclude<Mixed, "none">;
type Handlers = Record<`on${string}`, number>;
type Props = { title: string; onClick: number; onKey: number };
type IsHandler<T> = T extends `on${string}` ? "yes" : "no";

function handle(name: Handler): string {
  return name.slice(2);
}
function size(value: Px): number {
  return parseFloat(value);
}
function kind(value: Mixed): string {
  switch (value) {
    case "none":
      return "empty";
    default:
      return `color ${value}`;
  }
}

export function main(): void {
  const click: Handler = "onClick";
  const key = "Key";
  const built: Handler = `on${key}`;
  const dynamic = String(3);
  const fromString: Handler = `on${dynamic}`;
  const width: Px = "12.5px";
  const id: Id = "id-42";
  const loud: Loud = "HELLO";
  const title: Title = "Hello world";
  const event: Event = "ONCLICK";
  const colors: Mixed[] = ["none", "#fff"];
  const wide: string = click;
  console.log(`${handle(click)} ${handle(built)} ${fromString} ${size(width)} ${id} ${loud}`);
  console.log(`${title} ${event} ${colors.join(",")} ${wide.length}`);
  let current: Handler = "onA";
  current = "onB";
  const a: IsHandler<"onX"> = "yes";
  const b: IsHandler<"x"> = "no";
  const c: IsHandler<Handler> = "yes";
  const d: IsHandler<`on${number}`> = "yes";
  console.log(`${current === "onB"} ${a} ${b} ${c} ${d}`);
  const handlers: Handlers = { onClick: 1 };
  handlers["onKey"] = 2;
  const color: OnlyColor = "#000";
  const maybe: Mixed = "#abc";
  console.log(`${handlers.onClick} ${handlers["onKey"]} ${color} ${kind(maybe)} ${kind("none")}`);
  const numbers: `${number}`[] = ["1", "2.5", "1e3"];
  const plain: Omit<Props, `on${string}`> = { title: "t" };
  console.log(`${numbers.map((x) => Number(x)).join(",")} ${JSON.stringify(plain)}`);
}
