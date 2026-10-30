interface Readable<T> {
  readonly value: T;
  label: string;
}
interface Signal<T> extends Readable<T> {
  count: number;
}
interface Kind {
  kind: string;
}
type Shape = Readable<number> | Kind;

function first<T>(r: Readable<T>): T {
  return r.value;
}
function asReadable(s: Signal<number>): Readable<number> {
  return s;
}
function describe(shape: Shape): string {
  if ("kind" in shape) return `kind ${shape.kind}`;
  return `readable ${shape.label}`;
}
class Box {
  label = "box";
  value = 5;
  kind = "boxed";
}
interface Labeled {
  label: string;
}
function relabel(l: Labeled, text: string): void {
  l.label = text;
}

export function main(): void {
  const s: Signal<number> = { value: 1, label: "s", count: 2 };
  console.log(`${first(s)} ${first<number>(s)}`);
  const r = asReadable(s);
  console.log(`${r.label} ${r === s}`);
  let current: Readable<number> = s;
  const b = new Box();
  current = b;
  console.log(`${current.label} ${current.value}`);
  const shapes: Shape[] = [s, { kind: "k" }, b];
  console.log(shapes.map((x) => describe(x)).join(","));
  relabel(b, "renamed");
  relabel(s, "s2");
  console.log(`${b.label} ${s.label} ${JSON.stringify(s)} ${JSON.stringify(b)}`);
  const maybe: Readable<number> | undefined = shapes.length > 1 ? s : undefined;
  console.log(`${maybe?.label} ${maybe?.value}`);
  const copy = current;
  console.log(JSON.stringify(copy));
  console.log(`${current instanceof Box} ${r instanceof Box}`);
  const many: Readable<number>[] = [];
  for (let i = 0; i < 50; i++) {
    const item: Signal<number> = { value: i, label: `n${i}`, count: i * 2 };
    many.push(item);
  }
  let sum = 0;
  for (const item of many) sum += item.value + item.label.length;
  console.log(`${sum}`);
}
