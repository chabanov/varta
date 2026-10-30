// Unions of several record shapes, classes and arrays: discriminant, `in`,
// `instanceof`, `Array.isArray` and `typeof` narrowing, field reads and method
// calls on the union, exhaustive `never`, object literals selected by their
// discriminant, and JSON through checked assertions.
type Message =
  | { kind: "user"; text: string }
  | { kind: "chunk"; index: number; text: string }
  | { kind: "done" };
interface Item { id: string; n: number }
interface Page { items: Item[]; total: number }
interface Single { item: Item }
class Circle {
  readonly r: number;
  constructor(r: number) {
    this.r = r;
  }
  area(): number {
    return 3 * this.r * this.r;
  }
}
class Square {
  side: number;
  constructor(side: number) {
    this.side = side;
  }
  area(): number {
    return this.side * this.side;
  }
}
interface Envelope { id: string; event: { kind: "a"; x: number } | { kind: "b"; y: string } }
function describe(m: Message): string {
  switch (m.kind) {
    case "user":
      return "user " + m.text;
    case "chunk":
      return "chunk " + String(m.index) + " " + m.text;
    case "done":
      return "done";
    default: {
      const unreachable: never = m;
      return unreachable;
    }
  }
}
function text(m: Message): string {
  if (m.kind === "done") return "-";
  return m.kind + ":" + m.text;
}
function count(f: Page | Single): number {
  if ("items" in f) return f.total + f.items.length;
  return f.item.n;
}
function area(s: Circle | Square): number {
  if (s instanceof Circle) return s.area() + s.r;
  return s.area() + s.side;
}
function first(v: string | Item[]): string {
  if (Array.isArray(v)) return String(v.length) + v[0].id;
  return v;
}
function kind(v: string[] | number[]): string {
  return typeof v[0] === "string" ? "strings " + String(v.length) : "numbers " + String(v.length);
}
function envelope(e: Envelope): string {
  if (e.event.kind === "a") return e.id + String(e.event.x);
  return e.id + e.event.y;
}
export async function main(): Promise<void> {
  const all: Message[] = [{ kind: "user", text: "hi" }, { kind: "chunk", index: 2, text: "x" }, { kind: "done" }];
  for (const m of all) console.log(describe(m) + " | " + text(m));
  await Promise.resolve();
  console.log(String(count({ items: [{ id: "a", n: 1 }], total: 5 })) + " " + String(count({ item: { id: "b", n: 7 } })));
  const shapes: (Circle | Square)[] = [new Circle(2), new Square(3)];
  let total = 0;
  for (const s of shapes) total += s.area();
  console.log(String(area(shapes[0])) + " " + String(area(shapes[1])) + " " + String(total));
  await Promise.resolve();
  console.log(first([{ id: "z", n: 0 }]) + " " + first("plain") + " " + kind(["a"]) + " " + kind([1, 2]));
  console.log(envelope({ id: "1", event: { kind: "a", x: 4 } }) + envelope({ id: "2", event: { kind: "b", y: "why" } }));
  const parsed = JSON.parse('{"id":"3","event":{"kind":"b","y":"parsed"}}') as Envelope;
  await Promise.resolve();
  console.log(envelope(parsed) + " " + JSON.stringify(parsed));
  const raw: unknown = JSON.parse('{"kind":"chunk","index":9,"text":"t"}');
  console.log(describe(raw as Message));
  console.log(JSON.stringify(all));
}
