const calls: string[] = [];
interface Sized {
  readonly size: number;
  name: string;
}
class Box {
  item: Sized;
  constructor(item: Sized) {
    this.item = item;
  }
}
function sized(name: string): Sized {
  return {
    name,
    get size(): number {
      calls.push(name);
      return name.length;
    },
  };
}
function show(value: unknown): void {
  console.log(JSON.stringify(value));
}
export function main(): void {
  const a = sized("alpha");
  show(a);
  show([a, sized("be")]);
  show({ inner: a, list: [a] });
  show(new Box(sized("boxed")));
  console.log(calls.join(","));
  calls.length = 0;
  const held: unknown = a;
  const view = held as Record<string, unknown>;
  console.log(Object.keys(view).join(","));
  console.log(String(view["size"]));
  console.log(calls.join(","));
  if (typeof held === "object" && held !== null && "size" in held) {
    console.log("has size");
  }
}
