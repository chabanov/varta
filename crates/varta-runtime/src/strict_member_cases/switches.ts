// Exhaustive `switch` statements without `default` over a discriminant path:
// interfaces, inline object types and a nested path; each clause still
// narrows its object, and a `never` binding checks the fallthrough.
interface A { kind: "a" }
interface B { kind: "b"; n: number }
type U = A | B;
function f(u: U): string {
  switch (u.kind) {
    case "a": return "a";
    case "b": return "b" + String(u.n);
  }
}
type Shape = { kind: "circle"; r: number } | { kind: "square"; side: number } | { kind: "dot" };
function area(s: Shape): number {
  switch (s.kind) {
    case "circle": return 3 * s.r * s.r;
    case "square": return s.side * s.side;
    case "dot": return 0;
  }
}
interface Envelope { id: string; payload: { type: "text"; text: string } | { type: "count"; count: number } }
function open(e: Envelope): string {
  switch (e.payload.type) {
    case "text": return e.id + ":" + e.payload.text;
    case "count": return e.id + "#" + String(e.payload.count);
  }
}
function label(u: U): string {
  let out = "";
  switch (u.kind) {
    case "a":
      out = "first";
      break;
    case "b":
      out = "second " + String(u.n);
      break;
    default: {
      const unreachable: never = u;
      return unreachable;
    }
  }
  return out;
}
export function main(): void {
  console.log(f({ kind: "b", n: 1 }) + " " + f({ kind: "a" }));
  console.log(String(area({ kind: "circle", r: 2 })) + " " + String(area({ kind: "square", side: 3 })) + " " + String(area({ kind: "dot" })));
  console.log(open({ id: "x", payload: { type: "text", text: "hi" } }) + " " + open({ id: "y", payload: { type: "count", count: 4 } }));
  console.log(label({ kind: "a" }) + " " + label({ kind: "b", n: 2 }));
  const parsed = JSON.parse('{"id":"z","payload":{"type":"count","count":9}}') as Envelope;
  console.log(open(parsed));
}
