// JSON.parse returns unknown: typeof, null and Array.isArray guards narrow it,
// `in` proves keys of a JSON object, and a checked assertion reads it as a
// Record<string, unknown>.
function describe(value: unknown): string {
  if (typeof value === "object" && value !== null && !Array.isArray(value)) {
    if ("status" in value) {
      return "problem " + String(value.status);
    }
    return "object " + JSON.stringify(Object.keys(value));
  }
  return Array.isArray(value) ? "array" : typeof value;
}
export function main(): void {
  console.log(describe(JSON.parse('{"status":410,"title":"Gone"}')));
  console.log(describe(JSON.parse('{"b":[1,"x",null],"a":{}}')));
  console.log(describe(JSON.parse("[true]")) + " " + describe(JSON.parse("null")) + " " + describe(JSON.parse("2.5")));
  const record = JSON.parse('{"k":[1,{"n":null}]}') as Record<string, unknown>;
  console.log(JSON.stringify(record["k"]) + " " + String(record["missing"]));
}
