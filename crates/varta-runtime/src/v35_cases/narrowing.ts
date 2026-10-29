// Narrowing unknown to object by typeof/null/truth guards and to its keys by in.
function describe(value: unknown): string {
  if (typeof value === "object" && value !== null && !Array.isArray(value)) {
    const keys = Object.keys(value);
    if ("detail" in value && "status" in value) {
      const detail = value.detail;
      const status = value.status;
      return "problem " + JSON.stringify(keys) + " " + String(detail) + " " + String(status);
    }
    let names = "";
    for (const key in value) {
      names = names + key + ";";
    }
    return "object " + names + " " + String("title" in value) + " " + String(Object.hasOwn(value, "x"));
  }
  if (Array.isArray(value)) {
    return "array";
  }
  return typeof value;
}
function truthy(value: unknown): string {
  if (value && typeof value === "object") {
    return JSON.stringify(Object.entries(value));
  }
  return "falsy or primitive";
}
function reversed(value: unknown): string {
  if (value !== null && typeof value === "object" && "a" in value) {
    return String(value.a);
  }
  return "-";
}
export function main(): void {
  console.log(describe(JSON.parse('{"status":404,"detail":"gone","x":1}')));
  console.log(describe(JSON.parse('{"x":1,"y":[2]}')));
  console.log(describe(JSON.parse("[1,2]")));
  console.log(describe(JSON.parse("null")));
  console.log(describe(JSON.parse('"s"')));
  console.log(truthy(JSON.parse('{"k":true,"n":null}')) + " " + truthy(JSON.parse("0")));
  console.log(reversed(JSON.parse('{"a":"yes"}')) + " " + reversed(JSON.parse('{"b":1}')) + " " + reversed(JSON.parse("null")));
}
