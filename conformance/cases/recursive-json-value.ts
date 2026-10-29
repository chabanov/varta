// A recursive JSON value alias through arrays and an index signature, narrowed
// with Array.isArray and typeof.
type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
function count(value: JsonValue): number {
  if (Array.isArray(value)) {
    let total = 1;
    for (const item of value) {
      if (item !== undefined) {
        total = total + count(item);
      }
    }
    return total;
  }
  if (typeof value === "object" && value !== null) {
    let total = 1;
    for (const key of Object.keys(value)) {
      const item = key === undefined ? undefined : value[key];
      if (item !== undefined) {
        total = total + count(item);
      }
    }
    return total;
  }
  return 1;
}
export function main(): void {
  const doc: JsonValue = { a: [1, "two", { b: null }], c: true };
  console.log(String(count(doc)));
  console.log(JSON.stringify(doc));
}
