// keyof with precise T[K], typeof queries, keyof typeof and intersections.
interface Point {
  x: number;
  y: number;
  label: string;
}
const defaults = { width: 640, height: 480, title: "main" };
const palette = { red: "#f00", green: "#0f0" } as const;
const mode = "dark";
function area(width: number, height: number): number {
  return width * height;
}

type Settings = typeof defaults;
type Color = keyof typeof palette;
type Mode = typeof mode;
type Area = typeof area;
type Coordinate = Point["x" | "y"];
type Named = { name: string };
type Aged = { age: number; name: string };
type Person = Named & Aged;
type Tagged = { kind: "a" | "b" } & { kind: "a" | "c"; size?: number };

function get(point: Point, key: keyof Point): number | string {
  return point[key];
}
function coordinate(point: Point, key: "x" | "y"): Coordinate {
  return point[key];
}

export function main(): void {
  const point: Point = { x: 3, y: 4, label: "p" };
  console.log(`${get(point, "label")} ${coordinate(point, "y") + 1}`);
  const settings: Settings = { width: 800, height: 600, title: "copy" };
  console.log(`${settings.width}x${settings.height} ${settings.title} ${defaults.title}`);
  const colors: Color[] = ["red", "green"];
  console.log(colors.map((color) => palette[color]).join(" "));
  const current: Mode = "dark";
  const compute: Area = area;
  console.log(`${current} ${compute(6, 7)}`);
  const person: Person = { name: "Ada", age: 36 };
  console.log(`${person.name} ${person.age}`);
  const tagged: Tagged = { kind: "a" };
  console.log(`${tagged.kind} ${tagged.size}`);
  const keys: (keyof Person)[] = ["age", "name"];
  console.log(keys.join(","));
}
