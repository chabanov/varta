// switch on strings, literal unions and typeof, with fallthrough.
type Kind = "event" | "data" | "id" | "retry";
function field(name: string): number {
  let r = 0;
  switch (name) {
    case "event":
      r = 1;
      break;
    case "data":
    case "id":
      r = 2;
    case "retry": {
      r = r + 10;
      break;
    }
    default:
      r = 99;
  }
  return r;
}
function kind(k: Kind): string {
  switch (k) {
    case "event":
      return "E";
    case "data":
      return "D";
    default:
      return "other:" + k;
  }
}
function describe(x: number | string | boolean | undefined): string {
  switch (typeof x) {
    case "number":
      return "number " + String(x + 1);
    case "string":
      return "string " + x.slice(1);
    case "undefined":
      return "undefined";
    default:
      return "boolean " + String(x);
  }
}
interface Log { calls: string }
function key(trace: Log, name: string): string {
  trace.calls += name + ";";
  return name;
}
function order(v: string): string {
  const trace: Log = { calls: "" };
  switch (v) {
    default:
      trace.calls += "default;";
      break;
    case key(trace, "a"):
      trace.calls += "A;";
      break;
    case key(trace, "b"):
      trace.calls += "B;";
  }
  return trace.calls;
}
export function main(): void {
  console.log(String(field("event")) + " " + String(field("data")) + " " + String(field("id")) + " " + String(field("retry")) + " " + String(field("zz")));
  console.log(kind("event") + kind("data") + kind("id"));
  console.log(describe(1) + "|" + describe("ABC") + "|" + describe(undefined) + "|" + describe(true));
  console.log(order("b") + " " + order("zz"));
}
