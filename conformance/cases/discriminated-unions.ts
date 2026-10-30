// A union of record shapes narrows by its literal discriminant in `switch`
// and `if`, reads fields every member declares, and is exhaustive: the
// default clause sees `never`.
type Shape =
  | { kind: "circle"; r: number }
  | { kind: "rect"; w: number; h: number }
  | { kind: "point" };
function area(s: Shape): number {
  switch (s.kind) {
    case "circle":
      return 3 * s.r * s.r;
    case "rect":
      return s.w * s.h;
    case "point":
      return 0;
    default: {
      const unreachable: never = s;
      return unreachable;
    }
  }
}
function label(s: Shape): string {
  if (s.kind === "point") return s.kind;
  return s.kind + "!";
}
export function main(): void {
  const shapes: Shape[] = [{ kind: "circle", r: 1 }, { kind: "rect", w: 2, h: 3 }, { kind: "point" }];
  let out = "";
  for (const s of shapes) out += String(area(s)) + label(s) + " ";
  console.log(out);
}
